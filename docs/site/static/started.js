// A started page's download (docs/specs/site.md §5.3): a moment after the
// page loads, the file. GitHub serves it as an attachment, so the page
// stays. Without JavaScript, the <noscript> refresh does the same; nothing
// here waits on the analytics beacon, which an ad blocker may stop.
(function () {
  "use strict";
  var link = document.getElementById("file-link");
  if (!link) return;
  function go() {
    window.setTimeout(function () { window.location.href = link.href; }, 400);
  }
  if (document.readyState === "complete") go();
  else window.addEventListener("load", go);
})();
