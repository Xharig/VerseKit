// "Pass it on" button for the referral link: share sheet on touch devices, copy elsewhere.
(function () {
  var knopf = document.getElementById('ref-teilen');
  if (!knopf) return;
  var meldung = document.getElementById('ref-meldung');
  var link = knopf.getAttribute('data-link');
  var code = knopf.getAttribute('data-code');

  function englisch() { return document.documentElement.lang === 'en'; }
  function sagen(text) { meldung.textContent = text; }

  function kopieren() {
    if (!navigator.clipboard) { sagen(link); return; }
    navigator.clipboard.writeText(link).then(
      function () { sagen(englisch() ? 'Link copied. Paste it wherever you like.' : 'Link kopiert. Füg ihn ein, wo du willst.'); },
      function () { sagen(link); });
  }

  knopf.addEventListener('click', function () {
    if (navigator.share && window.matchMedia('(pointer: coarse)').matches) {
      navigator.share({
        title: 'Star Citizen',
        text: englisch()
          ? 'New to Star Citizen? With the referral code ' + code + ' you get 50,000 UEC after buying a game package.'
          : 'Neu in Star Citizen? Mit dem Empfehlungscode ' + code + ' bekommst du nach dem Kauf eines Spielpakets 50.000 UEC dazu.',
        url: link
      }).catch(function () {});
      return;
    }
    kopieren();
  });
})();
