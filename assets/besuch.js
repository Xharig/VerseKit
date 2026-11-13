// Counts a visit that came through a tagged link (versekit.xharig.com/?ref=discord).
// Reads ?ref= once on load and, only if it is a valid tag, asks the counter to
// add one for that tag and day. No tag, nothing is sent. Nothing is stored in
// the browser (no cookie, no storage) and no address or ID goes along.
// The tag rule is the same as in tools/nutzung-worker/worker.js (REF_RE).
(function () {
  try {
    var ref = new URLSearchParams(window.location.search).get('ref');
    if (!ref || !/^[a-z]{1,20}$/.test(ref)) return;
    // no-cors: the answer is not needed, so no CORS headers are required.
    fetch('https://nutzung-versekit.xharig.com/besuch?ref=' + ref, {
      method: 'GET',
      mode: 'no-cors',
      credentials: 'omit',
      cache: 'no-store',
      referrerPolicy: 'no-referrer',
      keepalive: true
    }).catch(function () {});
  } catch (e) {
    // The counter must never break the page.
  }
})();
