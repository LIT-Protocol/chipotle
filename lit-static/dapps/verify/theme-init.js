// Set the theme before first paint to avoid a flash of the wrong theme.
// Kept in its own file (not inline) so the page can ship a CSP that forbids
// inline scripts — inline script is the main vector for forging the verdict.
(function () {
  try {
    var t = localStorage.getItem('verify-theme');
    if (t !== 'light' && t !== 'dark') {
      t = window.matchMedia && window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
    }
    document.documentElement.setAttribute('data-theme', t);
  } catch (e) {
    document.documentElement.setAttribute('data-theme', 'dark');
  }
})();
