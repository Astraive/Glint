'use strict';

// Bundled panel script (SPK-001 spike). Talks to the host only through
// window.chrome.webview.postMessage with fixed string messages.
const host = window.chrome && window.chrome.webview;
const toggle = document.getElementById('cc-toggle');
const clock = document.getElementById('clock');

function post(message) {
  if (host) host.postMessage(message);
}

document.addEventListener('securitypolicyviolation', (event) => {
  post('csp-violation:' + event.effectiveDirective + ' ' + event.blockedURI);
});

toggle.addEventListener('click', () => post('toggle-control-center'));
document.getElementById('quit').addEventListener('click', () => post('quit'));

if (host) {
  host.addEventListener('message', (event) => {
    if (event.data === 'control-center:open') toggle.setAttribute('aria-expanded', 'true');
    if (event.data === 'control-center:closed') toggle.setAttribute('aria-expanded', 'false');
  });
}

function tick() {
  clock.textContent = new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}
tick();
setInterval(tick, 15000);

post('panel-ready');
