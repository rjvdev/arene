// Graphiques de l'Arène (méthode dataviz) : un seul axe par graphique, couleurs par rôle (variables CSS),
// survol avec info-bulle, légende dès deux séries. Partagé par le tableau de bord et la comparaison.
window.Arene = (function () {
  const NS = "http://www.w3.org/2000/svg";
  const SLOTS = ["--s1", "--s2", "--s3", "--s4", "--s5", "--s6", "--s7", "--s8"];
  const tip = () => document.getElementById("tip");

  const esc = s => String(s == null ? "" : s).replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
  const f2 = v => (v == null || isNaN(v)) ? "–" : (Math.round(v * 100) / 100).toFixed(2);
  const entier = v => String(Math.round(v));

  function montrer(ev, html) {
    const t = tip(); if (!t) return;
    t.innerHTML = html; t.style.display = "block";
    const w = t.offsetWidth, h = t.offsetHeight;
    let x = ev.clientX + 14, y = ev.clientY + 14;
    if (x + w > innerWidth - 8) x = ev.clientX - w - 14;
    if (y + h > innerHeight - 8) y = ev.clientY - h - 14;
    t.style.left = x + "px"; t.style.top = y + "px";
  }
  function cacher() { const t = tip(); if (t) t.style.display = "none"; }

  function el(tag, attrs, parent) {
    const e = document.createElementNS(NS, tag);
    for (const k in attrs) e.setAttribute(k, attrs[k]);
    if (parent) parent.appendChild(e);
    return e;
  }
  function txt(parent, x, y, s, cls, anchor) {
    const t = el("text", { x, y, class: cls, "text-anchor": anchor || "middle" }, parent);
    t.textContent = s; return t;
  }
  function versSvg(svg, ev) {
    const p = svg.createSVGPoint(); p.x = ev.clientX; p.y = ev.clientY;
    return p.matrixTransform(svg.getScreenCTM().inverse());
  }
  const pastille = coul => `<span style="display:inline-block;width:9px;height:9px;border-radius:2px;margin-right:5px;background:var(${coul})"></span>`;

  // Cadre commun : axe Y unique, graduations, étiquettes X. Dessin à la largeur réelle (texte non réduit).
  // bandes = true pour les barres : chaque génération occupe une case, rien ne déborde sur l'axe.
  function cadre(svg, etiquettes, ymin, ymax, fmt, bandes) {
    svg.innerHTML = "";
    const W = Math.max(260, Math.round(svg.parentElement.getBoundingClientRect().width - 28)) || 600;
    const H = 200, L = 38, R = 10, T = 10, B = 26;
    svg.setAttribute("viewBox", `0 0 ${W} ${H}`);
    const n = etiquettes.length, iw = W - L - R, ih = H - T - B;
    const pas = bandes ? iw / Math.max(1, n) : (n <= 1 ? 0 : iw / (n - 1));
    const x = bandes ? (i => L + pas * (i + 0.5)) : (i => L + (n <= 1 ? iw / 2 : i * pas));
    const y = v => T + ih - ((v - ymin) / ((ymax - ymin) || 1)) * ih;
    [ymin, (ymin + ymax) / 2, ymax].forEach(t => {
      el("line", { x1: L, y1: y(t), x2: W - R, y2: y(t), class: "grid" }, svg);
      txt(svg, L - 7, y(t) + 4, fmt(t), "tick", "end");
    });
    el("line", { x1: L, y1: T + ih, x2: W - R, y2: T + ih, class: "axis" }, svg);
    const saut = Math.max(1, Math.ceil(n / Math.max(3, Math.floor(iw / 48))));
    for (let i = 0; i < n; i++) if (i % saut === 0 || i === n - 1) txt(svg, x(i), T + ih + 17, etiquettes[i], "tick");
    return { W, H, L, R, T, ih, iw, x, y, pas, n, bandes };
  }

  function indiceSouris(svg, c, ev) {
    if (!c.n) return -1;
    const p = versSvg(svg, ev);
    if (c.bandes) return Math.max(0, Math.min(c.n - 1, Math.floor((p.x - c.L) / c.pas)));
    return c.n <= 1 ? 0 : Math.max(0, Math.min(c.n - 1, Math.round((p.x - c.L) / c.pas)));
  }

  // Une ou plusieurs courbes sur le même axe. series : [{libelle, coul, valeurs:[nombre|null]}]
  function courbes(id, etiquettes, series, ymin, ymax, fmt, opts) {
    opts = opts || {};
    const svg = document.getElementById(id); if (!svg) return;
    const c = cadre(svg, etiquettes, ymin, ymax, fmt);
    series.forEach(s => {
      const pts = s.valeurs.map((v, i) => v == null ? null : [c.x(i), c.y(v)]);
      let d = "", leve = true;
      pts.forEach(p => { if (!p) { leve = true; return; } d += (leve ? "M" : "L") + p[0].toFixed(1) + "," + p[1].toFixed(1); leve = false; });
      if (d) el("path", { d, class: "serie", style: `stroke:var(${s.coul})` }, svg);
      if (c.n <= 40) pts.forEach(p => p && el("circle", { cx: p[0], cy: p[1], r: 4, class: "pt", style: `fill:var(${s.coul})` }, svg));
    });
    const croix = el("line", { class: "cross", y1: c.T, y2: c.T + c.ih, visibility: "hidden" }, svg);
    const zone = el("rect", { x: c.L - 6, y: c.T, width: c.iw + 12, height: c.ih, fill: "transparent", style: opts.clic ? "cursor:pointer" : "" }, svg);
    zone.addEventListener("mousemove", ev => {
      const i = indiceSouris(svg, c, ev); if (i < 0) return;
      croix.setAttribute("x1", c.x(i)); croix.setAttribute("x2", c.x(i)); croix.setAttribute("visibility", "visible");
      const lignes = series.map(s => (series.length > 1 ? pastille(s.coul) + esc(s.libelle) + " : " : esc(s.libelle) + " : ") + "<b>" + (s.valeurs[i] == null ? "–" : fmt(s.valeurs[i])) + "</b>").join("<br>");
      montrer(ev, `<b>${esc(etiquettes[i])}</b>${opts.sousTitre ? ' <span class="m">· ' + esc(opts.sousTitre(i)) + "</span>" : ""}<br>${lignes}`);
    });
    zone.addEventListener("mouseleave", () => { croix.setAttribute("visibility", "hidden"); cacher(); });
    if (opts.clic) zone.addEventListener("click", ev => { const i = indiceSouris(svg, c, ev); if (i >= 0) opts.clic(i); });
    if (opts.legende) legende(opts.legende, series);
  }

  function legende(id, series, rond) {
    const lg = document.getElementById(id); if (!lg) return;
    lg.innerHTML = series.map(s => `<span><span class="sw${rond ? " rond" : ""}" style="background:var(${s.coul})"></span>${esc(s.libelle)}</span>`).join("");
  }

  // Barres empilées. series : [{libelle, coul}] ; valeur(i, s) -> nombre
  function barres(id, legendeId, etiquettes, series, valeur, ymax, opts) {
    opts = opts || {};
    const svg = document.getElementById(id); if (!svg) return;
    const haut = Math.max(1, ymax);
    const c = cadre(svg, etiquettes, 0, haut, entier, true);
    const larg = Math.max(3, Math.min(34, c.pas * 0.7));
    const hl = el("rect", { class: "col-hl", y: c.T, height: c.ih, width: larg + 8, visibility: "hidden" }, svg);
    for (let i = 0; i < c.n; i++) {
      let cumul = 0;
      series.forEach(s => {
        const v = valeur(i, s); if (!v) return;
        const y0 = c.y(cumul), y1 = c.y(cumul + v);
        el("rect", { x: c.x(i) - larg / 2, y: y1 + 1, width: larg, height: Math.max(0.5, y0 - y1 - 2), rx: 1.5, style: `fill:var(${s.coul})` }, svg);
        cumul += v;
      });
      const zl = Math.max(larg, 12) + 6;
      const zone = el("rect", { x: c.x(i) - zl / 2, y: c.T, width: zl, height: c.ih, fill: "transparent", style: opts.clic ? "cursor:pointer" : "" }, svg);
      zone.addEventListener("mousemove", ev => {
        hl.setAttribute("x", c.x(i) - larg / 2 - 4); hl.setAttribute("visibility", "visible");
        montrer(ev, `<b>${esc(etiquettes[i])}</b>${opts.sousTitre ? ' <span class="m">· ' + esc(opts.sousTitre(i)) + "</span>" : ""}<br>` +
          series.map(s => pastille(s.coul) + esc(s.libelle) + " : <b>" + valeur(i, s) + "</b>").join("<br>"));
      });
      zone.addEventListener("mouseleave", () => { hl.setAttribute("visibility", "hidden"); cacher(); });
      if (opts.clic) zone.addEventListener("click", () => opts.clic(i));
    }
    legende(legendeId, series);
  }

  // Couleur fixe par modèle (ordre alphabétique : stable d'un rendu à l'autre)
  function seriesModeles(gens) {
    const noms = [];
    gens.forEach(g => g.agents.forEach(a => { if (!noms.includes(a.modele)) noms.push(a.modele); }));
    noms.sort();
    return noms.map((nom, i) => ({ cle: nom, libelle: nom, coul: SLOTS[i % SLOTS.length] }));
  }

  // Arbre généalogique : une colonne par génération, liens parent -> enfant, clic = lignée.
  function arbre(id, legendeId, gens) {
    const svg = document.getElementById(id); if (!svg) return;
    svg.innerHTML = ""; svg.classList.remove("focus");
    const series = seriesModeles(gens), coulDe = {};
    series.forEach(s => coulDe[s.cle] = s.coul);
    const colW = 66, rowH = 24, padL = 26, padT = 26;
    const maxA = Math.max(1, ...gens.map(x => x.agents.length));
    const W = padL + Math.max(1, gens.length) * colW, H = padT + maxA * rowH + 8;
    svg.setAttribute("viewBox", `0 0 ${W} ${H}`);
    svg.style.width = Math.max(W, 300) + "px";
    const pos = {};
    gens.forEach((gen, gi) => {
      const x = padL + gi * colW + colW / 2 - 16;
      if (gi % Math.max(1, Math.ceil(gens.length / 20)) === 0 || gi === gens.length - 1) txt(svg, x, 14, "G" + gen.gen, "gl");
      gen.agents.forEach((a, ri) => { pos[gi + ":" + a.id] = { x, y: padT + ri * rowH + rowH / 2, a, gen }; });
    });
    const entrants = {}, sortants = {}, aretes = [];
    gens.forEach((gen, gi) => {
      if (gi === 0) return;
      const prec = new Set(gens[gi - 1].agents.map(a => a.id));
      gen.agents.forEach(a => {
        const dest = gi + ":" + a.id;
        const sources = prec.has(a.id) ? [[a.id, true]] : (a.parents || []).filter(p => prec.has(p)).map(p => [p, false]);
        sources.forEach(([pid, cont]) => {
          const src = (gi - 1) + ":" + pid, A = pos[src], B = pos[dest];
          if (!A || !B) return;
          const mx = (A.x + B.x) / 2;
          const p = el("path", { d: `M${A.x},${A.y} C${mx},${A.y} ${mx},${B.y} ${B.x},${B.y}`, class: "edge" + (cont ? " cont" : "") }, svg);
          aretes.push({ src, dest, p });
          (entrants[dest] = entrants[dest] || []).push(src);
          (sortants[src] = sortants[src] || []).push(dest);
        });
      });
    });
    const noeuds = {};
    for (const cle in pos) {
      const { x, y, a, gen } = pos[cle];
      const vivant = a.verdict === "survivant";
      const n = el("circle", { cx: x, cy: y, r: vivant ? 6.5 : 5, class: "node" + (vivant ? "" : " mort"),
        style: vivant ? `fill:var(${coulDe[a.modele]})` : `stroke:var(${coulDe[a.modele]})` }, svg);
      noeuds[cle] = n;
      n.addEventListener("mousemove", ev => montrer(ev,
        `<b>#${a.id}</b> <span class="m">· G${gen.gen} · ${esc(a.modele)}</span><br>` +
        `${esc(a.choix)}${a.choix === "COMBAT" ? (a.correct ? ' <span class="ok">✓ juste</span>' : ' <span class="ko">✗ faux</span>') : ""} · confiance ${f2(a.confiance)} · score ${a.score}<br>` +
        (a.reponse ? `réponse : <b>${esc(a.reponse)}</b><br>` : "") +
        ((a.trace || []).length ? `🔧 ${a.trace.length} appel(s) d'outil : ${[...new Set(a.trace.map(e => e.outil))].map(esc).join(", ")}<br>` : (a.outils ? '<span class="m">outils autorisés, non utilisés</span><br>' : "")) +
        (a.testament ? `<i>« ${esc(a.testament)} »</i><br>` : "") +
        `<b>${vivant ? "survivant" : "mort"}</b> · risque ${f2(a.risque)} · T° ${f2(a.temperature)}<br>` +
        `<span class="m">${(a.principes || []).map(esc).join("<br>")}</span>` +
        (a.parents && a.parents.length ? `<br><span class="m">parents : ${a.parents.map(p => "#" + p).join(", ")}</span>` : "")));
      n.addEventListener("mouseleave", cacher);
      n.addEventListener("click", ev => {
        ev.stopPropagation();
        const lignee = new Set([cle]);
        const marcher = carte => { const pile = [cle]; while (pile.length) { const k = pile.pop(); (carte[k] || []).forEach(v => { if (!lignee.has(v)) { lignee.add(v); pile.push(v); } }); } };
        marcher(entrants); marcher(sortants);
        svg.classList.add("focus");
        for (const k in noeuds) noeuds[k].classList.toggle("hl", lignee.has(k));
        aretes.forEach(e => e.p.classList.toggle("hl", lignee.has(e.src) && lignee.has(e.dest)));
      });
    }
    svg.onclick = () => {
      svg.classList.remove("focus");
      for (const k in noeuds) noeuds[k].classList.remove("hl");
      aretes.forEach(e => e.p.classList.remove("hl"));
    };
    const lg = document.getElementById(legendeId);
    if (lg) lg.innerHTML = series.map(s => `<span><span class="sw rond" style="background:var(${s.coul})"></span>${esc(s.libelle)}</span>`).join("") +
      `<span><span class="sw rond" style="background:var(--muted)"></span>survivant</span><span><span class="sw rond creux"></span>mort</span>`;
  }

  // ---------------- carte d'agent (tableau de bord, page Résultats)
  const pct = v => Math.round((v || 0) * 100);
  function jugement(gen, a) {
    const piege = gen.genre === "PIÈGE";
    if (a.choix === "COMBAT") {
      if (piege) return '<span class="ko">✗ a répondu à un piège</span>';
      return a.correct ? '<span class="ok">✓ juste</span>' : `<span class="ko">✗ faux</span>${gen.attendu ? ' <span class="aide">(attendu ' + esc(gen.attendu) + ")</span>" : ""}`;
    }
    if (a.choix === "REFUS") return piege ? '<span class="ok">✓ a déjoué le piège</span>' : '<span class="ko">✗ a refusé une question faisable</span>';
    if (a.cassee) return '<span class="ko">réponse vide ou illisible : comptée comme abandon</span>';
    if (a.choix === "ABANDON") return '<span class="aide">abandon honnête</span>';
    return "";
  }

  function carteAgent(gen, a, nSurv) {
    const vivant = a.verdict === "survivant";
    const ancien = a.reponse === undefined;   // runs enregistrés avant que les réponses soient gardées
    const rep = a.choix === "COMBAT" && !a.cassee
      ? (ancien ? '<span class="aide">réponse non enregistrée (run ancien)</span>'
         : `<span class="${(a.reponse || "").length > 24 ? "rep-long" : "rep"}">${esc(a.reponse || "∅")}</span>`)
      : "";
    return `<article class="agent${vivant ? " survivant" : ""}">
      <div class="tete"><strong>Agent #${a.id}</strong><span class="verdict">${vivant ? '<span class="ok">✓ survit</span>' : '<span class="aide">✝ meurt</span>'} · score ${a.score}</span></div>
      ${!vivant && a.correct ? `<div class="aide">Juste, mais hors du top ${nSurv || 3} : départagé par la confiance.</div>` : ""}
      ${a.partage_avec ? `<div class="aide">Même génome que l'agent #${a.partage_avec} : sa réponse a été réutilisée (calcul économisé).</div>` : ""}
      <div class="ligne-rep"><span class="choix">${esc(a.choix)}</span>${rep}<span class="jugement">${jugement(gen, a)}</span></div>
      <div class="conf"><span>confiance ${pct(a.confiance)} %</span><span class="barre"><div style="width:${pct(a.confiance)}%"></div></span></div>
      ${a.cassee ? `<div class="aide">${esc(a.testament)}</div>` : a.testament ? `<blockquote>« ${esc(a.testament)} »</blockquote>` : (ancien ? "" : '<span class="aide">pas de testament</span>')}
      ${a.cassee && a.reponse ? `<details><summary class="aide">Texte brut renvoyé</summary><div class="rep-long">${esc(a.reponse)}</div></details>` : ""}
      ${a.pensee ? `<details><summary class="aide">Réflexion de Qwen (${a.pensee.length} caractères)</summary><div class="rep-long" style="font-size:13px; color:var(--ink2)">${esc(a.pensee)}</div></details>` : ""}
      ${a.contexte ? `<div class="meta${a.contexte.coupe ? " ko" : ""}">Contexte : ${a.contexte.prompt_max} jetons de prompt · ${a.contexte.genere} générés · fenêtre ${a.contexte.fenetre}${a.contexte.appels > 1 ? " · " + a.contexte.appels + " appels" : ""} · ${a.contexte.coupe ? "⚠ coupé avant la fin" : "✓ réflexion menée à terme"}</div>` : ""}
      ${(a.trace || []).length ? `<details class="trace"><summary>🔧 ${a.trace.length} appel${a.trace.length > 1 ? "s" : ""} d'outil : ${[...new Set(a.trace.map(e => e.outil))].map(esc).join(", ")}</summary>${a.trace.map(e => `<div class="etape"><div><strong>${esc(e.outil)}</strong></div>${e.entree ? `<pre>${esc(e.entree)}</pre>` : ""}<div class="aide">→ résultat</div><pre>${esc(e.sortie)}</pre></div>`).join("")}</details>` : ""}
      <div class="meta">${a.outils ? "🔧 outils autorisés · " : ""}${esc(a.modele)} · risque ${f2(a.risque)} · T° ${f2(a.temperature)}${a.parents && a.parents.length ? " · parents " + a.parents.map(p => "#" + p).join(", ") : ""}<br>${(a.principes || []).map(esc).join(" · ")}</div>
    </article>`;
  }

  return { esc, f2, entier, courbes, barres, arbre, seriesModeles, SLOTS, jugement, carteAgent, pct };
})();
