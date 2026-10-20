(() => {
  const notice = new URLSearchParams(location.search).get('notice');
  if (notice) {
    const element = document.createElement('div'); element.className = 'notice'; element.role = 'status'; element.textContent = notice;
    document.body.append(element);
    setTimeout(() => element.remove(), 5500);
  }
})();
