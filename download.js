(() => {
  const RELEASES_API =
    "https://api.github.com/repos/Anurag-error404/lumora/releases/latest";
  const RELEASES_PAGE =
    "https://github.com/Anurag-error404/lumora/releases/latest";

  /** Prefer these installers for non-technical users. */
  const PREFERRED_EXT = {
    macos: [".dmg"],
    windows: ["-setup.exe", ".exe", ".msi"],
    linux: [".appimage", ".deb", ".rpm"],
  };

  const PLATFORM_LABEL = {
    macos: "Mac",
    windows: "Windows",
    linux: "Linux",
  };

  const INSTALL_STEPS = {
    macos: [
      {
        title: "Open the .dmg file",
        body: "Find it in Downloads and double-click. A window with the LUMORA icon appears.",
      },
      {
        title: "Drag LUMORA into Applications",
        body: "Drop the app icon onto the Applications folder in that window.",
      },
      {
        title: "Open LUMORA",
        body: "Launch it from Applications. If macOS blocks it, go to System Settings → Privacy & Security and choose Open Anyway.",
      },
    ],
    windows: [
      {
        title: "Open the setup file",
        body: "Find LUMORA’s .exe in Downloads and double-click it.",
      },
      {
        title: "Allow the installer",
        body: "If Windows asks whether to allow changes, choose Yes, then follow the short setup wizard.",
      },
      {
        title: "Launch LUMORA",
        body: "Open it from the Start menu, then choose Import photos and pick a folder.",
      },
    ],
    linux: [
      {
        title: "Make the AppImage runnable",
        body: "Right-click the downloaded file → Properties → Permissions → allow executing as a program (or: chmod +x on the file).",
      },
      {
        title: "Double-click to run",
        body: "Open the AppImage. Some desktops ask you to integrate it — that’s optional.",
      },
      {
        title: "Import your photos",
        body: "In LUMORA, choose Import photos and point it at a folder on your disk.",
      },
    ],
    mobile: [
      {
        title: "Switch to a computer",
        body: "LUMORA is a desktop app. Open this page on the Mac, Windows, or Linux machine where your photos live.",
      },
      {
        title: "Download there",
        body: "We’ll suggest the right installer for that computer automatically.",
      },
      {
        title: "Come back to the guide",
        body: "After installing, the guide walks you through importing a folder.",
      },
    ],
    unknown: [
      {
        title: "Open the file",
        body: "Find it in your Downloads folder and double-click it.",
      },
      {
        title: "Follow the installer",
        body: "Accept the defaults unless you prefer another location.",
      },
      {
        title: "Launch LUMORA",
        body: "Then choose Import photos and pick a folder on your disk.",
      },
    ],
  };

  const el = {
    status: document.getElementById("download-status"),
    primary: document.getElementById("primary-download"),
    version: document.getElementById("download-version"),
    hint: document.getElementById("download-hint"),
    region: document.getElementById("download-primary"),
    stepsList: document.getElementById("install-steps-list"),
    stepsIntro: document.getElementById("steps-intro"),
    platformList: document.getElementById("platform-list"),
    checksumRow: document.getElementById("checksum-row"),
    checksumValue: document.getElementById("checksum-value"),
    checksumCopy: document.getElementById("checksum-copy"),
    checksumFallback: document.getElementById("checksum-fallback"),
    releaseNotes: document.getElementById("release-notes"),
    releaseNotesEmpty: document.getElementById("release-notes-empty"),
    whatsNewHeading: document.getElementById("whats-new-heading"),
    whatsNewSub: document.getElementById("whats-new-sub"),
    statLatest: document.getElementById("stat-latest"),
    statDate: document.getElementById("stat-date"),
    statDownloads: document.getElementById("stat-downloads"),
    statNotesLink: document.getElementById("stat-notes-link"),
  };

  function sha256FromDigest(digest) {
    if (!digest || typeof digest !== "string") return "";
    const m = /^(?:sha256:)?([a-f0-9]{64})$/i.exec(digest.trim());
    return m ? m[1].toLowerCase() : "";
  }

  function setChecksum(asset) {
    const hash = asset ? sha256FromDigest(asset.digest) : "";
    if (hash && el.checksumRow && el.checksumValue) {
      el.checksumRow.hidden = false;
      el.checksumValue.textContent = hash;
      if (el.checksumFallback) el.checksumFallback.hidden = true;
    } else {
      if (el.checksumRow) el.checksumRow.hidden = true;
      if (el.checksumFallback) el.checksumFallback.hidden = false;
    }
  }

  function stripMd(text) {
    return String(text || "")
      .replace(/\r\n/g, "\n")
      .replace(/^#{1,6}\s+/gm, "")
      .replace(/\*\*([^*]+)\*\*/g, "$1")
      .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
      .trim();
  }

  function setReleaseNotes(data) {
    const tag = data.tag_name || data.name || "";
    const body = stripMd(data.body || "");
    if (el.whatsNewHeading && tag) {
      el.whatsNewHeading.textContent = `What’s New in ${tag}`;
    }
    if (el.releaseNotes && body) {
      const truncated = body.length > 1800 ? `${body.slice(0, 1800).trim()}…` : body;
      el.releaseNotes.hidden = false;
      el.releaseNotes.textContent = truncated;
      if (el.releaseNotesEmpty) el.releaseNotesEmpty.hidden = true;
    }
    if (el.whatsNewSub && data.html_url) {
      el.whatsNewSub.innerHTML = `From the latest GitHub release · <a href="${data.html_url}" rel="noopener noreferrer">full notes</a>`;
    }
  }

  function setStats(data, downloadTotal) {
    const tag = data.tag_name || data.name || "—";
    if (el.statLatest) el.statLatest.textContent = tag;
    if (el.statDate && data.published_at) {
      try {
        el.statDate.textContent = new Date(data.published_at).toLocaleDateString(
          undefined,
          { year: "numeric", month: "short", day: "numeric" }
        );
      } catch {
        el.statDate.textContent = data.published_at.slice(0, 10);
      }
    }
    if (el.statDownloads && typeof downloadTotal === "number" && downloadTotal > 0) {
      el.statDownloads.textContent = downloadTotal.toLocaleString();
    }
    if (el.statNotesLink && data.html_url) {
      el.statNotesLink.href = data.html_url;
    }
  }

  if (el.checksumCopy && el.checksumValue) {
    el.checksumCopy.addEventListener("click", async () => {
      const text = el.checksumValue.textContent || "";
      if (!text) return;
      try {
        await navigator.clipboard.writeText(text);
        el.checksumCopy.textContent = "Copied";
        setTimeout(() => {
          el.checksumCopy.textContent = "Copy checksum";
        }, 1500);
      } catch {
        el.checksumCopy.textContent = "Copy failed";
      }
    });
  }

  function detectPlatform() {
    const ua = navigator.userAgent || "";
    const platform = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || "";
    const p = `${platform} ${ua}`.toLowerCase();

    const isMobile =
      /iphone|ipad|ipod|android|mobile/i.test(ua) ||
      (navigator.userAgentData && navigator.userAgentData.mobile);

    if (isMobile) return "mobile";
    if (/mac|darwin|iphone|ipad/.test(p) && !/iphone|ipad|ipod/.test(ua.toLowerCase())) {
      // Desktop Mac (UA may still say "MacIntel" on Apple silicon).
      return "macos";
    }
    if (/win/.test(p)) return "windows";
    if (/linux|x11|cros/.test(p)) return "linux";
    return "unknown";
  }

  function pickAsset(assets, platform) {
    const names = assets
      .map((a) => ({
        name: a.name,
        url: a.browser_download_url,
        size: a.size,
        digest: a.digest || "",
        download_count: a.download_count || 0,
      }))
      .filter((a) => !a.name.endsWith(".sig") && a.name !== "latest.json");

    const prefs = PREFERRED_EXT[platform] || [];
    for (const ext of prefs) {
      const match = names.find((a) => a.name.toLowerCase().endsWith(ext.toLowerCase()));
      if (match) return match;
    }
    return null;
  }

  function assetsByPlatform(assets) {
    return {
      macos: pickAsset(assets, "macos"),
      windows: pickAsset(assets, "windows"),
      linux: pickAsset(assets, "linux"),
    };
  }

  function formatBytes(bytes) {
    if (!bytes || bytes <= 0) return "";
    const mb = bytes / (1024 * 1024);
    return mb >= 10 ? `${Math.round(mb)} MB` : `${mb.toFixed(1)} MB`;
  }

  function setSteps(platform) {
    const steps = INSTALL_STEPS[platform] || INSTALL_STEPS.unknown;
    if (!el.stepsList) return;
    el.stepsList.innerHTML = steps
      .map(
        (s) =>
          `<li><strong>${s.title}</strong><span>${s.body}</span></li>`
      )
      .join("");
  }

  function wirePlatformLinks(byPlatform, versionLabel) {
    if (!el.platformList) return;
    el.platformList.querySelectorAll("[data-platform]").forEach((link) => {
      const key = link.getAttribute("data-platform");
      const asset = byPlatform[key];
      const meta = link.querySelector("[data-meta]");
      if (asset) {
        link.href = asset.url;
        link.removeAttribute("aria-disabled");
        link.classList.remove("is-unavailable");
        if (meta) {
          const size = formatBytes(asset.size);
          const base =
            key === "macos"
              ? "Apple silicon (M1 and newer) · .dmg"
              : key === "windows"
                ? "64-bit PC · setup wizard"
                : "64-bit · AppImage";
          meta.textContent = size ? `${base} · ${size}` : base;
        }
      } else {
        link.href = RELEASES_PAGE;
        if (meta) meta.textContent = "See all files on GitHub Releases";
      }
      if (versionLabel) {
        link.setAttribute("download", "");
        link.setAttribute("data-version", versionLabel);
      }
    });
  }

  function markRecommended(platform) {
    if (!el.platformList) return;
    el.platformList.querySelectorAll(".platform-option").forEach((link) => {
      const isMatch = link.getAttribute("data-platform") === platform;
      link.classList.toggle("is-recommended", isMatch);
      if (isMatch) {
        link.setAttribute("aria-current", "true");
      } else {
        link.removeAttribute("aria-current");
      }
    });
  }

  function showPrimary({ platform, asset, versionLabel }) {
    if (!el.primary || !el.status) return;

    if (platform === "mobile") {
      el.status.textContent = "You’re on a phone or tablet";
      el.primary.textContent = "See desktop downloads";
      el.primary.href = "#all-platforms";
      el.primary.removeAttribute("download");
      if (el.hint) {
        el.hint.hidden = false;
        el.hint.textContent =
          "LUMORA runs on Mac, Windows, and Linux computers — not on phones. Scroll down to grab the file for your desktop.";
      }
      if (el.stepsIntro) {
        el.stepsIntro.textContent =
          "Install on the computer where you keep your photo library.";
      }
      setChecksum(null);
      setSteps("mobile");
      return;
    }

    if (asset) {
      const label = PLATFORM_LABEL[platform] || "your computer";
      el.status.textContent = `Ready for ${label}`;
      el.primary.textContent = `Download for ${label}`;
      el.primary.href = asset.url;
      el.primary.setAttribute("download", "");
      if (el.hint) {
        el.hint.hidden = false;
        const size = formatBytes(asset.size);
        el.hint.textContent = size
          ? `${asset.name} · ${size}`
          : asset.name;
      }
      setChecksum(asset);
    } else {
      el.status.textContent = "Choose your system below";
      el.primary.textContent = "Browse all downloads";
      el.primary.href = RELEASES_PAGE;
      el.primary.setAttribute("rel", "noopener noreferrer");
      if (el.hint) {
        el.hint.hidden = false;
        el.hint.textContent =
          "We couldn’t auto-match an installer — pick Mac, Windows, or Linux below.";
      }
      setChecksum(null);
    }

    if (el.version && versionLabel) {
      el.version.hidden = false;
      el.version.textContent = `Latest version ${versionLabel}`;
    }

    if (el.stepsIntro) {
      el.stepsIntro.textContent =
        platform === "macos"
          ? "On a Mac: open the disk image, drag the app, then launch it."
          : platform === "windows"
            ? "On Windows: run the setup wizard, then open LUMORA from the Start menu."
            : platform === "linux"
              ? "On Linux: make the AppImage executable, then double-click to run."
              : "Three short steps. No account, no cloud signup.";
    }

    setSteps(platform in INSTALL_STEPS ? platform : "unknown");
    markRecommended(platform);
  }

  async function init() {
    const platform = detectPlatform();
    setSteps(platform in INSTALL_STEPS ? platform : "unknown");

    try {
      const res = await fetch(RELEASES_API, {
        headers: { Accept: "application/vnd.github+json" },
      });
      if (!res.ok) throw new Error(`GitHub API ${res.status}`);
      const data = await res.json();
      const versionLabel = (data.tag_name || data.name || "").replace(/^v/, "");
      const byPlatform = assetsByPlatform(data.assets || []);
      wirePlatformLinks(byPlatform, versionLabel);
      setReleaseNotes(data);

      let downloadTotal = 0;
      for (const asset of data.assets || []) {
        downloadTotal += asset.download_count || 0;
      }
      setStats(data, downloadTotal);

      const asset =
        platform === "mobile" || platform === "unknown"
          ? null
          : byPlatform[platform];

      showPrimary({ platform, asset, versionLabel });
    } catch {
      if (el.status) el.status.textContent = "Downloads are on GitHub";
      if (el.primary) {
        el.primary.textContent = "Open latest release";
        el.primary.href = RELEASES_PAGE;
      }
      if (el.hint) {
        el.hint.hidden = false;
        el.hint.textContent =
          "We couldn’t load the file list automatically. The release page has Mac, Windows, and Linux installers.";
      }
      showPrimary({ platform, asset: null, versionLabel: "" });
      wirePlatformLinks({}, "");
      markRecommended(platform);
    } finally {
      if (el.region) el.region.setAttribute("aria-busy", "false");
    }
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
