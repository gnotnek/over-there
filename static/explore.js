(() => {
  const element = document.getElementById('explore-map');
  if (!element) return;
  const status = document.getElementById('near-status');
  const button = document.getElementById('near-me');
  const near = element.dataset.near ? element.dataset.near.split(',').map(Number) : null;

  // Location is rounded to about 1 km before it leaves the browser.
  // An automatic jump replaces the history entry so Back still leaves Explore.
  const locate = (auto = false) => {
    if (!navigator.geolocation) { status.textContent = "Couldn't find your location. Showing newest routes."; return; }
    if (button) button.disabled = true;
    navigator.geolocation.getCurrentPosition(position => {
      const url = new URL(location.href);
      url.searchParams.set('near', `${position.coords.latitude.toFixed(2)},${position.coords.longitude.toFixed(2)}`);
      url.searchParams.set('sort', 'nearest');
      url.searchParams.delete('page');
      if (auto) location.replace(url); else location.assign(url);
    }, error => {
      if (button) button.disabled = false;
      status.textContent = error.code === error.PERMISSION_DENIED
        ? 'Location is off, so routes are shown newest first.'
        : "Couldn't find your location. Showing newest routes.";
    }, {timeout: 10000, maximumAge: 600000});
  };
  if (button) {
    button.addEventListener('click', () => locate());
    // Only a bare visit to Explore may use an already-granted location; explicit filters and sorts are left alone.
    if (location.search === '' && navigator.permissions?.query) {
      navigator.permissions.query({name: 'geolocation'}).then(result => { if (result.state === 'granted') locate(true); }).catch(() => {});
    }
  }

  if (!window.maplibregl) {
    element.querySelector('.map-fallback').textContent = 'The map could not load. The routes are listed below.';
    return;
  }
  const pins = JSON.parse(element.dataset.pins || '[]');
  const map = new maplibregl.Map({container: element, style: element.dataset.style, center: near ? [near[1], near[0]] : [0, 20], zoom: near ? 9 : 1, attributionControl: true});
  map.addControl(new maplibregl.NavigationControl({showCompass: false}), 'top-right');
  map.on('load', () => element.querySelector('.map-fallback')?.remove());

  const cards = new Map([...document.querySelectorAll('.route-card[data-slug]')].map(card => [card.dataset.slug, card]));
  const dots = new Map();
  pins.forEach(pin => {
    const dot = document.createElement('button');
    dot.type = 'button'; dot.className = 'map-point'; dot.title = pin.title; dot.setAttribute('aria-label', `Show ${pin.title} in the list`);
    dot.addEventListener('click', () => {
      const card = cards.get(pin.slug);
      if (!card) return;
      cards.forEach(other => other.classList.toggle('is-active', other === card));
      card.scrollIntoView({behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth', block: 'center'});
    });
    new maplibregl.Marker({element: dot}).setLngLat([pin.lon, pin.lat]).addTo(map);
    dots.set(pin.slug, dot);
  });
  cards.forEach((card, slug) => {
    card.addEventListener('mouseenter', () => dots.get(slug)?.classList.add('is-active'));
    card.addEventListener('mouseleave', () => dots.get(slug)?.classList.remove('is-active'));
  });

  const points = pins.map(pin => [pin.lon, pin.lat]);
  if (near) {
    const you = document.createElement('div'); you.className = 'map-you'; you.title = 'You (approximate)';
    new maplibregl.Marker({element: you}).setLngLat([near[1], near[0]]).addTo(map);
    points.push([near[1], near[0]]);
  }
  if (points.length) {
    const bounds = new maplibregl.LngLatBounds(points[0], points[0]);
    points.forEach(point => bounds.extend(point));
    map.fitBounds(bounds, {padding: 55, maxZoom: 14, duration: 0});
  }
})();
