'use strict';

(function () {
  var PREFIX = 'zafe-invite-';
  var KEY = 'zafe-invite';
  var URL_SAFE = /^[A-Za-z0-9._~:-]+$/;

  function valid(invite) {
    return typeof invite === 'string' && invite.indexOf(PREFIX) === 0 &&
      invite.length < 4096 && !/\s/.test(invite);
  }

  function fromFragment() {
    var hash = location.hash.slice(1);
    if (!hash) return null;
    try {
      return decodeURIComponent(hash).trim();
    } catch (e) {
      return null;
    }
  }

  function remember(invite) {
    try { sessionStorage.setItem(KEY, invite); } catch (e) { }
  }

  function remembered() {
    try { return sessionStorage.getItem(KEY); } catch (e) { return null; }
  }

  var invite = fromFragment();
  if (valid(invite)) {
    remember(invite);
  } else if (invite === null) {
    invite = remembered();
  }
  if (location.hash) history.replaceState(null, '', location.pathname);

  if (!valid(invite)) {
    document.getElementById('missing').hidden = false;
    return;
  }

  var value = URL_SAFE.test(invite) ? invite : encodeURIComponent(invite);
  document.getElementById('open').href = 'zafe://join?invite=' + value;

  var status = document.getElementById('copied');
  document.getElementById('copy').addEventListener('click', function () {
    function done() { status.textContent = 'Copied. Paste it on Zafe\'s Join screen.'; }
    function failed() { status.textContent = 'Couldn\'t copy. Paste the whole link instead.'; }
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(invite).then(done, failed);
    } else {
      failed();
    }
  });

  document.getElementById('invite').hidden = false;
})();
