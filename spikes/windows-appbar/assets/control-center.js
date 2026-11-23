'use strict';

// Bundled control-center script (SPK-001 spike).
const host = window.chrome && window.chrome.webview;

function post(message) {
  if (host) host.postMessage(message);
}

document.addEventListener('securitypolicyviolation', (event) => {
  post('csp-violation:' + event.effectiveDirective + ' ' + event.blockedURI);
});

document.getElementById('close').addEventListener('click', () => post('close-control-center'));
document.addEventListener('keydown', (event) => {
  if (event.key === 'Escape') post('close-control-center');
});

post('control-center-ready');
