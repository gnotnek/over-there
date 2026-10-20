(() => {
  const element = document.getElementById('draw-map');
  if (!element) return;
  const form = element.closest('form');
  const submit = document.getElementById('draw-submit');
  const status = document.getElementById('draw-status');
  const stats = document.getElementById('draw-stats');
  const follow = document.getElementById('draw-follow');
  const locate = document.getElementById('draw-locate');
  const routing = element.dataset.routing;
  document.getElementById('draw-credit').textContent = `Routing by BRouter. The points you place are sent to ${new URL(routing).host}.`;
  if (!window.maplibregl) {
    element.querySelector('.map-fallback').textContent = 'The map could not load, so a route cannot be drawn right now.';
    return;
  }
  const profiles = {road: 'fastbike', gravel: 'gravel', mountain: 'mtb'};
  const profile = () => profiles[form.elements.cycling_type?.value || element.dataset.cyclingType] || 'trekking';

  // points[k] is a placed point [lon, lat]; legs[k] joins points[k] to points[k + 1].
  let points = [];
  let legs = [];
  if (element.dataset.plan) {
    const line = JSON.parse(element.dataset.line);
    const plan = JSON.parse(element.dataset.plan);
    points = plan.points;
    legs = plan.straight.map((straight, k) => ({coords: line.slice(plan.at[k], plan.at[k + 1] + 1), straight}));
  }

  const route = async (a, b) => {
    if (follow.checked) {
      try {
        const url = `${routing}?lonlats=${a[0]},${a[1]}|${b[0]},${b[1]}&profile=${profile()}&alternativeidx=0&format=geojson`;
        const response = await fetch(url, {signal: AbortSignal.timeout(10000)});
        if (!response.ok) throw new Error(response.statusText);
        const coords = (await response.json()).features[0].geometry.coordinates;
        if (coords.length > 1) return {coords, straight: false};
      } catch (error) { /* fall through to a straight leg */ }
      status.textContent = "Couldn't follow roads here, so this part is a straight line.";
    }
    return {coords: [a.slice(0, 2), b.slice(0, 2)], straight: true};
  };
  // Routing snaps points to the road, so placed points follow the legs' ends.
  const snap = () => {
    legs.forEach((leg, k) => { points[k] = leg.coords[0].slice(0, 2); });
    if (legs.length) points[legs.length] = legs.at(-1).coords.at(-1).slice(0, 2);
  };

  // One line without repeated joints; a joint keeps the elevation of whichever leg has one.
  const build = () => {
    if (!legs.length) return {line: [], plan: null};
    const line = [];
    const at = [];
    legs.forEach(leg => {
      if (!line.length) line.push(leg.coords[0]);
      else if (line.at(-1).length < 3 && leg.coords[0].length > 2) line[line.length - 1] = leg.coords[0];
      at.push(line.length - 1);
      line.push(...leg.coords.slice(1));
    });
    at.push(line.length - 1);
    return {line, plan: {points: at.map(i => line[i].slice(0, 2)), at, straight: legs.map(leg => leg.straight)}};
  };
  const haversine = (a, b) => {
    const rad = Math.PI / 180;
    const h = Math.sin((b[1] - a[1]) * rad / 2) ** 2 + Math.cos(a[1] * rad) * Math.cos(b[1] * rad) * Math.sin((b[0] - a[0]) * rad / 2) ** 2;
    return 2 * 6371000 * Math.asin(Math.sqrt(Math.min(1, h)));
  };
  // Same climbing rule as the server: the reference height moves only after a change of more than 3 m.
  const ascent = line => {
    let gain = 0;
    let reference = line.find(p => p.length > 2)?.[2];
    line.forEach(p => {
      if (p.length < 3 || reference === undefined) return;
      if (p[2] - reference > 3) { gain += p[2] - reference; reference = p[2]; } else if (reference - p[2] > 3) reference = p[2];
    });
    return gain;
  };

  const map = new maplibregl.Map({container: element, style: element.dataset.style, center: [0, 20], zoom: 1, attributionControl: true});
  map.addControl(new maplibregl.NavigationControl({showCompass: false}), 'top-right');
  let markers = [];
  const lines = straight => ({type: 'Feature', properties: {}, geometry: {type: 'MultiLineString', coordinates: legs.filter(leg => leg.straight === straight).map(leg => leg.coords)}});
  const render = () => {
    map.getSource('roads').setData(lines(false));
    map.getSource('straight').setData(lines(true));
    markers.forEach(marker => marker.remove());
    markers = points.map((point, k) => {
      const dot = document.createElement('button');
      dot.type = 'button';
      dot.className = `map-point${k === 0 ? ' map-start' : k === points.length - 1 ? ' map-finish' : ''}`;
      dot.setAttribute('aria-label', `Point ${k + 1}. Click to remove, drag to move.`);
      let dragged = false;
      const marker = new maplibregl.Marker({element: dot, draggable: true}).setLngLat(point).addTo(map);
      marker.on('dragstart', () => { dragged = true; });
      marker.on('dragend', () => { const {lng, lat} = marker.getLngLat(); queue(() => move(k, [lng, lat])); });
      dot.addEventListener('click', () => { if (!dragged) queue(() => remove(k)); dragged = false; });
      return marker;
    });
    const {line} = build();
    const distance = line.slice(1).reduce((sum, p, i) => sum + haversine(line[i], p), 0);
    stats.textContent = points.length
      ? `${(distance / 1000).toFixed(1)} km · ↑ ${Math.round(ascent(line))} m · ${points.length} ${points.length === 1 ? 'point' : 'points'}`
      : 'Click the map to place your start.';
    submit.disabled = legs.length === 0;
  };

  // Edits run one at a time, so a slow routing answer never lands on a newer line.
  let pending = Promise.resolve();
  const queue = edit => { pending = pending.then(async () => { status.textContent = ''; await edit(); snap(); render(); }); };
  const add = async point => {
    if (points.length >= 100) { status.textContent = 'A route can have at most 100 points.'; return; }
    points.push(point);
    if (points.length > 1) legs.push(await route(points.at(-2), point));
  };
  const move = async (k, point) => {
    points[k] = point;
    if (k > 0) legs[k - 1] = await route(points[k - 1], point);
    if (k < legs.length) legs[k] = await route(point, points[k + 1]);
  };
  const remove = async k => {
    points.splice(k, 1);
    if (k === 0) legs.shift();
    else if (k >= legs.length) legs.pop();
    else legs.splice(k - 1, 2, await route(points[k - 1], points[k]));
  };

  map.on('load', () => {
    element.querySelector('.map-fallback')?.remove();
    ['roads', 'straight'].forEach(id => map.addSource(id, {type: 'geojson', data: lines(id === 'straight')}));
    map.addLayer({id: 'roads-casing', type: 'line', source: 'roads', paint: {'line-color': '#C6FF2D', 'line-width': 9, 'line-opacity': .96}, layout: {'line-join': 'round', 'line-cap': 'round'}});
    map.addLayer({id: 'roads-line', type: 'line', source: 'roads', paint: {'line-color': '#0F0F0F', 'line-width': 5}, layout: {'line-join': 'round', 'line-cap': 'round'}});
    map.addLayer({id: 'straight-line', type: 'line', source: 'straight', paint: {'line-color': '#0F0F0F', 'line-width': 4, 'line-dasharray': [1.5, 1.5]}});
    render();
    const all = legs.flatMap(leg => leg.coords);
    if (all.length) {
      const bounds = new maplibregl.LngLatBounds(all[0].slice(0, 2), all[0].slice(0, 2));
      all.forEach(p => bounds.extend(p.slice(0, 2)));
      map.fitBounds(bounds, {padding: 55, maxZoom: 14, duration: 0});
    }
    map.on('click', event => {
      if (event.originalEvent.target.closest('.maplibregl-marker')) return;
      queue(() => add([event.lngLat.lng, event.lngLat.lat]));
    });
  });

  document.getElementById('draw-undo').addEventListener('click', () => queue(() => { points.pop(); legs.pop(); }));
  document.getElementById('draw-clear').addEventListener('click', () => queue(() => { points = []; legs = []; }));
  // The rider's location only moves the map; it is never sent anywhere.
  if (navigator.geolocation) {
    locate.addEventListener('click', () => navigator.geolocation.getCurrentPosition(
      position => map.flyTo({center: [position.coords.longitude, position.coords.latitude], zoom: 13}),
      () => { status.textContent = "Couldn't find your location. Pan the map to your area instead."; },
      {timeout: 10000, maximumAge: 600000}
    ));
  } else locate.hidden = true;

  form.addEventListener('submit', event => {
    if (!legs.length) { event.preventDefault(); return; }
    const {line, plan} = build();
    for (const [name, value] of [['line', line], ['plan', plan]]) {
      const files = new DataTransfer();
      files.items.add(new File([JSON.stringify(value)], `${name}.json`, {type: 'application/json'}));
      form.elements[name].files = files.files;
    }
  });
})();
