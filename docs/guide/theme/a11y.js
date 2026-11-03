// Accessibility fixes to mdBook's page template (docs/specs/site.md §6), as
// CI's axe-core check found them (2026-11-03). Without scripts, the page's
// landmarks and headings still let a screen reader find its way.
(function () {
  // "Skip to the content": the first thing a keyboard reaches on every page,
  // before the sidebar's links.
  var main = document.querySelector("main");
  if (main) {
    if (!main.id) main.id = "content-main";
    main.setAttribute("tabindex", "-1");
    var skip = document.createElement("a");
    skip.className = "skip-link";
    skip.href = "#" + main.id;
    skip.textContent = "Skip to the content";
    document.body.insertBefore(skip, document.body.firstChild);
  }

  // The sidebar's toggle is a <label> for a hidden checkbox, which mdBook
  // marks aria-expanded: a label can't carry that, and the keyboard can't
  // reach it. As a button it can do both; Enter and Space press it.
  var toggle = document.getElementById("mdbook-sidebar-toggle");
  if (toggle && toggle.tagName === "LABEL") {
    toggle.setAttribute("role", "button");
    toggle.setAttribute("tabindex", "0");
    toggle.setAttribute(
      "aria-expanded",
      document.documentElement.classList.contains("sidebar-visible") ? "true" : "false"
    );
    toggle.addEventListener("keydown", function (e) {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        toggle.click();
      }
    });
  }

  // A table wider than the page scrolls sideways in its wrapper; the
  // keyboard needs to reach the wrapper to scroll it, and a screen reader a
  // name for it (the heading above the table).
  function heading(el) {
    for (var n = el.previousElementSibling; n; n = n.previousElementSibling) {
      if (/^H[1-6]$/.test(n.tagName)) return n.textContent.trim();
    }
    return "Table";
  }
  function scrollables() {
    var wrappers = document.querySelectorAll(".table-wrapper");
    for (var i = 0; i < wrappers.length; i++) {
      var w = wrappers[i];
      if (w.scrollWidth > w.clientWidth + 1) {
        w.setAttribute("tabindex", "0");
        w.setAttribute("role", "region");
        w.setAttribute("aria-label", heading(w));
      } else if (w.getAttribute("role") === "region") {
        w.removeAttribute("tabindex");
        w.removeAttribute("role");
        w.removeAttribute("aria-label");
      }
    }
  }
  scrollables();
  window.addEventListener("resize", scrollables);
})();
