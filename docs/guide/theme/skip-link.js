// "Skip to the content": the first thing a keyboard reaches on every page,
// before the sidebar's links (docs/specs/site.md §6). Without scripts, the
// page's landmarks and headings still let a screen reader jump to it.
(function () {
  var main = document.querySelector("main");
  if (!main) return;
  if (!main.id) main.id = "content-main";
  main.setAttribute("tabindex", "-1");
  var a = document.createElement("a");
  a.className = "skip-link";
  a.href = "#" + main.id;
  a.textContent = "Skip to the content";
  document.body.insertBefore(a, document.body.firstChild);
})();
