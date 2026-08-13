(() => {
  const prefersReduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  // —— Reveal on scroll ——
  const nodes = document.querySelectorAll(".reveal");
  if (prefersReduced || !("IntersectionObserver" in window)) {
    nodes.forEach((el) => el.classList.add("is-visible"));
  } else {
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            entry.target.classList.add("is-visible");
            observer.unobserve(entry.target);
          }
        }
      },
      { rootMargin: "0px 0px -8% 0px", threshold: 0.12 }
    );
    nodes.forEach((el) => observer.observe(el));
  }

  // —— Hero search-result rotation ——
  const heroImg = document.querySelector("[data-hero-img]");
  const heroQuery = document.querySelector("[data-hero-query]");
  const heroFrames = [
    { src: "docs/screenshots/hero_search_black_dog.webp", query: "black dog" },
    { src: "docs/screenshots/hero_search_sunset.webp", query: "sunset" },
    { src: "docs/screenshots/hero_search_mountain_trail.webp", query: "mountain trail" },
  ];
  if (heroImg && !prefersReduced) {
    const applyHeroFrame = (frame) => {
      heroImg.src = frame.src;
      heroImg.alt = `LUMORA search results for “${frame.query}” from a local photo library`;
      if (heroQuery) heroQuery.textContent = frame.query;
    };
    const showHeroFrame = (frame, instant) => {
      // Decode first so a swap never paints a half-loaded frame.
      const next = new Image();
      next.src = frame.src;
      const paint = () => {
        if (instant) return applyHeroFrame(frame);
        heroImg.classList.add("is-swapping");
        setTimeout(() => {
          applyHeroFrame(frame);
          heroImg.classList.remove("is-swapping");
        }, 260);
      };
      next.decode ? next.decode().then(paint).catch(paint) : (next.onload = paint);
    };
    // Random start so repeat visitors don't always land on the same shot.
    let h = Math.floor(Math.random() * heroFrames.length);
    if (h > 0) showHeroFrame(heroFrames[h], true);
    setInterval(() => {
      h = (h + 1) % heroFrames.length;
      showHeroFrame(heroFrames[h]);
    }, 5200);
  }

  // —— Product demo screenshot cycle ——
  const demoImg = document.querySelector("[data-demo-img]");
  const demoFrames = [
    { src: "docs/screenshots/home.webp", alt: "LUMORA home library grid" },
    {
      src: "docs/screenshots/search_mountain_hike.webp",
      alt: "LUMORA search results for “mountain hike”",
    },
    {
      src: "docs/screenshots/search-black-dog.webp",
      alt: "LUMORA search results for “black dog”",
    },
  ];
  if (demoImg && demoFrames.length && !prefersReduced) {
    let i = 0;
    setInterval(() => {
      i = (i + 1) % demoFrames.length;
      const frame = demoFrames[i];
      demoImg.src = frame.src;
      demoImg.alt = frame.alt;
    }, 3200);
  }

  // —— Search playground ——
  const chips = document.querySelectorAll(".play-chip");
  const playImg = document.querySelector("[data-play-img]");
  const playQuery = document.querySelector("[data-play-query]");
  chips.forEach((chip) => {
    chip.addEventListener("click", () => {
      chips.forEach((c) => c.classList.remove("is-active"));
      chip.classList.add("is-active");
      const q = chip.getAttribute("data-query") || "";
      const src = chip.getAttribute("data-img") || "";
      if (playQuery) playQuery.textContent = q;
      if (playImg && src) {
        playImg.src = src;
        playImg.alt = `Example results for “${q}”`;
        playImg.classList.remove("play-flash");
        void playImg.offsetWidth;
        playImg.classList.add("play-flash");
      }
    });
  });

  // —— GitHub social proof (soft-fail) ——
  const proof = document.getElementById("github-proof");
  if (proof) {
    const starsEl = proof.querySelector('[data-stat="stars"]');
    const contribEl = proof.querySelector('[data-stat="contributors"]');
    const downloadsEl = proof.querySelector('[data-stat="downloads"]');

    const fmt = (n) => {
      if (typeof n !== "number" || !Number.isFinite(n)) return null;
      if (n >= 1000) return `${(n / 1000).toFixed(n >= 10000 ? 0 : 1)}k`;
      return String(n);
    };

    fetch("https://api.github.com/repos/Anurag-error404/lumora", {
      headers: { Accept: "application/vnd.github+json" },
    })
      .then((r) => (r.ok ? r.json() : null))
      .then((repo) => {
        if (!repo) return;
        if (starsEl && typeof repo.stargazers_count === "number") {
          starsEl.textContent = `★ ${fmt(repo.stargazers_count)}`;
        }
      })
      .catch(() => {});

    fetch("https://api.github.com/repos/Anurag-error404/lumora/contributors?per_page=1", {
      headers: { Accept: "application/vnd.github+json" },
    })
      .then((r) => {
        if (!r.ok || !contribEl) return null;
        const link = r.headers.get("Link") || "";
        const last = /[?&]page=(\d+)>;\s*rel="last"/.exec(link);
        if (last) {
          contribEl.textContent = `${last[1]} Contributors`;
        } else {
          return r.json().then((arr) => {
            if (Array.isArray(arr) && arr.length) {
              contribEl.textContent = `${arr.length} Contributor${arr.length === 1 ? "" : "s"}`;
            }
          });
        }
        return null;
      })
      .catch(() => {});

    fetch("https://api.github.com/repos/Anurag-error404/lumora/releases?per_page=10", {
      headers: { Accept: "application/vnd.github+json" },
    })
      .then((r) => (r.ok ? r.json() : null))
      .then((releases) => {
        if (!Array.isArray(releases) || !downloadsEl) return;
        let total = 0;
        for (const rel of releases) {
          for (const asset of rel.assets || []) {
            total += asset.download_count || 0;
          }
        }
        if (total > 0) downloadsEl.textContent = `${fmt(total)} Downloads`;
      })
      .catch(() => {});
  }

  // —— Click tracking (gtag / GA4) ——
  // Docs TOC + CTAs; soft-fails if gtag isn't loaded yet.
  const trackClick = (name, params) => {
    if (typeof window.gtag !== "function") return;
    window.gtag("event", name, params);
  };

  const clickLabel = (el) =>
    (el.getAttribute("aria-label") || el.textContent || "")
      .replace(/\s+/g, " ")
      .trim()
      .slice(0, 80);

  document.addEventListener("click", (e) => {
    const el = e.target.closest(
      [
        "a.btn",
        "button.btn",
        "button",
        "a.nav-cta",
        "a.star-sticky",
        "a.footer-kofi",
        "a.platform-option",
        ".guide-toc a",
        ".play-chip",
      ].join(",")
    );
    if (!el) return;

    const page_path = location.pathname;
    const link_text = clickLabel(el);
    const link_url = el.getAttribute("href") || "";

    if (el.matches(".guide-toc a")) {
      trackClick("docs_nav", { link_text, link_url, page_path });
      return;
    }
    if (el.matches(".play-chip")) {
      trackClick("demo_chip", {
        link_text: el.getAttribute("data-query") || link_text,
        page_path,
      });
      return;
    }

    let name = "cta_click";
    if (el.matches("a.footer-kofi")) {
      name = "donate_click";
    } else if (el.matches("a.platform-option") || el.classList.contains("btn-download") || el.id === "primary-download") {
      name = "download_click";
    } else if (el.matches("a.nav-cta, a.star-sticky")) {
      name = "github_cta";
    }

    trackClick(name, { link_text, link_url, page_path });
  });
})();
