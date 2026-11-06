// The download page's recommendation (docs/specs/site.md §5.2, §5.3).
// It reads what this browser says about the computer, here and nowhere
// else, and changes the page: it sets no cookie, stores nothing and sends
// nothing. Without it, the page lists every file with no recommendation.
(function () {
  "use strict";
  var FROMS = ["writers", "ai"];
  var builds = {};
  try {
    builds = JSON.parse(document.getElementById("builds").textContent);
  } catch (e) {
    builds = {};
  }

  // Which page sent the visitor, as a plain word in the started pages'
  // addresses, so downloads count per landing page (§5.3).
  var from = null;
  try {
    from = new URLSearchParams(window.location.search).get("from");
  } catch (e) {
    from = null;
  }
  if (FROMS.indexOf(from) >= 0) {
    var links = document.querySelectorAll("a[data-build]");
    for (var i = 0; i < links.length; i++) {
      links[i].setAttribute("href", links[i].getAttribute("href") + from + "/");
    }
  }

  // What the user agent string and the low-entropy hints say.
  function classify(ua, platform, touch, uadPlatform, uadMobile) {
    if (uadMobile || /Android|iPhone|iPad|iPod|Mobile|CrOS|Chrome OS|Chromium OS/i.test(ua) ||
        /ChromeOS|Chrome OS|Android/i.test(uadPlatform || "")) {
      return { os: "other" };
    }
    if (/Macintosh|Mac OS X/.test(ua) || /^Mac/.test(platform) || uadPlatform === "macOS") {
      // An iPad's Safari says it's a Mac, but a Mac has no touch screen.
      return touch > 1 ? { os: "other" } : { os: "macos" };
    }
    if (/Windows/.test(ua) || /^Win/.test(platform) || uadPlatform === "Windows") {
      return { os: "windows" };
    }
    if (/Linux|X11/.test(ua) || /Linux/.test(platform) || uadPlatform === "Linux") {
      var s = ua + " " + platform;
      var arch = /x86_64|amd64|x64/i.test(s) ? "x64" : /aarch64|arm64|armv8/i.test(s) ? "arm64" : null;
      return { os: "linux", arch: arch };
    }
    return { os: "unknown" };
  }

  // The build to recommend, or null (§5.2's table).
  function decide(base, hints) {
    var arch = null;
    if (hints && hints.architecture) {
      if (hints.architecture === "arm" && (hints.bitness === "64" || !hints.bitness)) arch = "arm64";
      else if (hints.architecture === "x86" && hints.bitness === "64") arch = "x64";
    }
    if (base.os === "macos") {
      if (builds["macos-universal"]) return "macos-universal";
      if (arch && builds["macos-" + arch]) return "macos-" + arch;
      return null;
    }
    if (base.os === "windows") {
      // Firefox and Safari say x64 on ARM computers too, so without the
      // hints there's no telling.
      return arch ? "windows-" + arch : null;
    }
    if (base.os === "linux") {
      var a = arch || base.arch;
      return a ? "linux-" + a + "-appimage" : null;
    }
    return null;
  }

  function hrefOf(id) {
    var a = document.querySelector('a[data-build="' + id + '"]');
    return a ? a.getAttribute("href") : null;
  }

  function firstGroup(os) {
    var section = document.querySelector('section[data-os="' + os + '"]');
    var list = document.getElementById("downloads");
    if (section && list && list.firstElementChild !== section) {
      list.insertBefore(section, list.firstElementChild);
    }
  }

  function show(base, id) {
    var b = id && builds[id];
    var href = id && hrefOf(id);
    if (b && href) {
      var link = document.getElementById("recommend-link");
      link.setAttribute("href", href);
      link.textContent = "Download: " + b.label.replace(/ \(.*\)$/, "");
      document.getElementById("recommend-detail").textContent = b.name + ", " + b.size;
      document.getElementById("recommend").hidden = false;
    }
    if (base.os === "other") {
      document.getElementById("not-a-computer").hidden = false;
      return;
    }
    if (base.os === "windows" || base.os === "macos" || base.os === "linux") {
      firstGroup(base.os);
    }
    if (!b && base.os === "windows") {
      var tell = document.getElementById("how-to-tell");
      tell.textContent = "Most Windows PCs need the x64 installer; Snapdragon laptops and the Surface Pro X need ARM64. Settings ▸ System ▸ About ▸ System type says which yours is.";
      tell.hidden = false;
    }
  }

  var ua = navigator.userAgent || "";
  var platform = navigator.platform || "";
  var touch = navigator.maxTouchPoints || 0;
  var uad = navigator.userAgentData;
  var base = classify(ua, platform, touch, uad && uad.platform, uad && uad.mobile);
  if (uad && uad.getHighEntropyValues && base.os !== "other" && base.os !== "unknown") {
    uad.getHighEntropyValues(["architecture", "bitness", "platform"]).then(
      function (h) { show(base, decide(base, h)); },
      function () { show(base, decide(base, null)); }
    );
  } else {
    show(base, decide(base, null));
  }
})();
