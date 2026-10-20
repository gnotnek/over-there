(() => {
  const element = document.getElementById('route-map');
  if (!element) return;
  if (!window.maplibregl) {
    element.querySelector('.map-fallback').textContent = 'The map could not load. The GPX is still available to download.';
    return;
  }
  const map = new maplibregl.Map({container: element, style: element.dataset.style, center: [121.5, 25], zoom: 8, attributionControl: true});
  map.addControl(new maplibregl.NavigationControl({showCompass: false}), 'top-right');
  map.addControl(new maplibregl.FullscreenControl(), 'top-right');
  let hoverMarker;
  fetch(`/routes/${encodeURIComponent(element.dataset.slug)}/geometry`, {credentials: 'same-origin'})
    .then(response => { if (!response.ok) throw new Error('Route could not load'); return response.json(); })
    .then(geometry => {
      const lines = geometry.coordinates.filter(line => line.length > 1);
      if (!lines.length) throw new Error('Route has no drawable line');
      map.on('load', () => {
        element.querySelector('.map-fallback')?.remove();
        map.addSource('route', {type: 'geojson', data: {type: 'Feature', properties: {}, geometry}});
        map.addLayer({id: 'route-casing', type: 'line', source: 'route', paint: {'line-color': '#C6FF2D', 'line-width': 9, 'line-opacity': .96}, layout: {'line-join': 'round', 'line-cap': 'round'}});
        map.addLayer({id: 'route-line', type: 'line', source: 'route', paint: {'line-color': '#0F0F0F', 'line-width': 5}, layout: {'line-join': 'round', 'line-cap': 'round'}});
        const points = lines.flat();
        const bounds = new maplibregl.LngLatBounds(points[0], points[0]);
        points.forEach(point => bounds.extend(point));
        map.fitBounds(bounds, {padding: 55, maxZoom: 14, duration: 0});
        const marker = (position, label, cls) => {
          const dot = document.createElement('div'); dot.className = `map-point ${cls}`; dot.title = label;
          new maplibregl.Marker({element: dot}).setLngLat(position).addTo(map);
        };
        marker(lines[0][0], 'Start', 'map-start');
        marker(lines.at(-1).at(-1), 'Finish', 'map-finish');
      });
    }).catch(error => { element.querySelector('.map-fallback').textContent = error.message; });

  const chart = document.getElementById('elevation-chart');
  if (chart) {
    const points = JSON.parse(chart.dataset.points || '[]');
    if (points.length > 1) {
      const sampled = points.filter((_, i) => i % Math.max(1, Math.ceil(points.length / 600)) === 0 || i === points.length - 1);
      const w = 1000, h = 230, inset = 16;
      const maxX = points.at(-1)[0] || 1;
      const elevations = sampled.map(point => point[1]);
      const minY = Math.min(...elevations), maxY = Math.max(...elevations);
      const yRange = Math.max(1, maxY - minY);
      const coordinates = sampled.map(point => [inset + (point[0] / maxX) * (w - 2 * inset), h - inset - ((point[1] - minY) / yRange) * (h - 2 * inset)]);
      const path = coordinates.map(([x, y], i) => `${i ? 'L' : 'M'}${x.toFixed(1)},${y.toFixed(1)}`).join(' ');
      chart.innerHTML = `<svg viewBox="0 0 ${w} ${h}" preserveAspectRatio="none" role="img" aria-label="Elevation from ${Math.round(minY)} to ${Math.round(maxY)} metres"><path d="${path} L${w-inset},${h} L${inset},${h} Z" fill="#F1FFD0"/><path d="${path}" fill="none" stroke="#0F0F0F" stroke-width="3" vector-effect="non-scaling-stroke"/><line id="elevation-cursor" x1="0" x2="0" y1="0" y2="${h}" stroke="#2F2F2F" stroke-width="2" opacity="0"/></svg><div class="chart-readout" aria-live="polite"></div>`;
      chart.addEventListener('pointermove', event => {
        const rect = chart.getBoundingClientRect();
        const distance = Math.max(0, Math.min(1, (event.clientX - rect.left) / rect.width)) * maxX;
        let lo = 0, hi = points.length - 1;
        while (lo < hi) { const mid = (lo + hi) >> 1; if (points[mid][0] < distance) lo = mid + 1; else hi = mid; }
        const point = points[lo];
        const cursor = chart.querySelector('#elevation-cursor');
        cursor.setAttribute('x1', inset + (point[0] / maxX) * (w - 2 * inset));
        cursor.setAttribute('x2', inset + (point[0] / maxX) * (w - 2 * inset));
        cursor.setAttribute('opacity', '1');
        chart.querySelector('.chart-readout').textContent = `${(point[0] / 1000).toFixed(1)} km · ${Math.round(point[1])} m`;
        if (!hoverMarker) {
          const dot = document.createElement('div'); dot.className = 'map-point map-hover';
          hoverMarker = new maplibregl.Marker({element: dot}).setLngLat([point[2], point[3]]).addTo(map);
        } else hoverMarker.setLngLat([point[2], point[3]]);
      });
      chart.addEventListener('pointerleave', () => {
        chart.querySelector('#elevation-cursor').setAttribute('opacity', '0');
        chart.querySelector('.chart-readout').textContent = '';
        if (hoverMarker) { hoverMarker.remove(); hoverMarker = null; }
      });
    }
  }
  document.getElementById('share-route')?.addEventListener('click', async () => {
    if (navigator.share) await navigator.share({title: document.title, url: location.href}).catch(() => {});
    else if (navigator.clipboard) { await navigator.clipboard.writeText(location.href); document.getElementById('share-route').textContent = 'Link copied'; }
  });
})();
