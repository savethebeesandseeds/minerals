import {
  isOfferActiveAt,
  normalizeSearchParams,
  parseRoute,
  routeHref,
  validateManifest,
  validateWorkerRequest,
  validateWorkerResponse,
} from "./app-core.mjs";
import { registerMineralsWebMcp } from "./webmcp.mjs";

const CATALOG_WORKER_REVISION = "c2542afda6bbade538ec5c4e7b3cbdd668f5ffef215439b113408f7a1f814d80";
const APP_BASE_PATH = new URL(".", import.meta.url).pathname;
const main = document.querySelector("#app-main");
const statusRegion = document.querySelector("#app-status");
const releaseSummary = document.querySelector("#release-summary");
const header = document.querySelector("[data-site-header]");
const navToggle = document.querySelector(".nav-toggle");
const primaryNav = document.querySelector("#primary-navigation");
const localeSelect = document.querySelector("#locale-select");
const mapModuleMeta = document.querySelector('meta[name="waajacu-map-module"]');

const SUPPORTED_LOCALES = new Set(["en", "es", "de", "fr", "cs", "zh", "ar", "pt", "hi", "ja"]);
const ENGLISH = Object.freeze({
  catalog: "Catalog", map: "Map", source: "Source", language: "Language",
  search: "Search", searchLabel: "Search minerals", searchHint: "Name, formula, family, or keyword",
  previous: "Previous", next: "Next", evidence: "Evidence", offers: "Offers", details: "Scientific profile",
  opening: "Opening the verified catalog…", failed: "The public catalog could not be opened.", retry: "Try again",
});
const TRANSLATIONS = Object.freeze({
  en: ENGLISH,
  es: { catalog: "Catálogo", map: "Mapa", source: "Fuente", language: "Idioma", search: "Buscar", searchLabel: "Buscar minerales", searchHint: "Nombre, fórmula, familia o palabra clave", previous: "Anterior", next: "Siguiente", evidence: "Evidencia", offers: "Ofertas", details: "Perfil científico", opening: "Abriendo el catálogo verificado…", failed: "No se pudo abrir el catálogo público.", retry: "Reintentar" },
  de: { catalog: "Katalog", map: "Karte", source: "Quelle", language: "Sprache", search: "Suchen", searchLabel: "Minerale suchen", searchHint: "Name, Formel, Familie oder Stichwort", previous: "Zurück", next: "Weiter", evidence: "Nachweise", offers: "Angebote", details: "Wissenschaftliches Profil", opening: "Verifizierter Katalog wird geöffnet…", failed: "Der öffentliche Katalog konnte nicht geöffnet werden.", retry: "Erneut versuchen" },
  fr: { catalog: "Catalogue", map: "Carte", source: "Source", language: "Langue", search: "Rechercher", searchLabel: "Rechercher des minéraux", searchHint: "Nom, formule, famille ou mot-clé", previous: "Précédent", next: "Suivant", evidence: "Sources", offers: "Offres", details: "Profil scientifique", opening: "Ouverture du catalogue vérifié…", failed: "Impossible d’ouvrir le catalogue public.", retry: "Réessayer" },
  cs: { catalog: "Katalog", map: "Mapa", source: "Zdroj", language: "Jazyk", search: "Hledat", searchLabel: "Hledat minerály", searchHint: "Název, vzorec, skupina nebo klíčové slovo", previous: "Předchozí", next: "Další", evidence: "Zdroje", offers: "Nabídky", details: "Vědecký profil", opening: "Otevírání ověřeného katalogu…", failed: "Veřejný katalog se nepodařilo otevřít.", retry: "Zkusit znovu" },
  zh: { catalog: "目录", map: "地图", source: "来源", language: "语言", search: "搜索", searchLabel: "搜索矿物", searchHint: "名称、化学式、类别或关键词", previous: "上一页", next: "下一页", evidence: "证据", offers: "报价", details: "科学档案", opening: "正在打开已验证目录…", failed: "无法打开公共目录。", retry: "重试" },
  ar: { catalog: "الفهرس", map: "الخريطة", source: "المصدر", language: "اللغة", search: "بحث", searchLabel: "البحث عن المعادن", searchHint: "الاسم أو الصيغة أو العائلة أو كلمة مفتاحية", previous: "السابق", next: "التالي", evidence: "الأدلة", offers: "العروض", details: "الملف العلمي", opening: "جارٍ فتح الكتالوج المتحقق منه…", failed: "تعذر فتح الكتالوج العام.", retry: "إعادة المحاولة" },
  pt: { catalog: "Catálogo", map: "Mapa", source: "Fonte", language: "Idioma", search: "Pesquisar", searchLabel: "Pesquisar minerais", searchHint: "Nome, fórmula, família ou palavra-chave", previous: "Anterior", next: "Seguinte", evidence: "Evidências", offers: "Ofertas", details: "Perfil científico", opening: "Abrindo o catálogo verificado…", failed: "Não foi possível abrir o catálogo público.", retry: "Tentar novamente" },
  hi: { catalog: "सूची", map: "मानचित्र", source: "स्रोत", language: "भाषा", search: "खोजें", searchLabel: "खनिज खोजें", searchHint: "नाम, सूत्र, परिवार या मुख्य शब्द", previous: "पिछला", next: "अगला", evidence: "साक्ष्य", offers: "प्रस्ताव", details: "वैज्ञानिक प्रोफ़ाइल", opening: "सत्यापित सूची खोली जा रही है…", failed: "सार्वजनिक सूची नहीं खोली जा सकी।", retry: "फिर प्रयास करें" },
  ja: { catalog: "カタログ", map: "地図", source: "ソース", language: "言語", search: "検索", searchLabel: "鉱物を検索", searchHint: "名前、化学式、分類、キーワード", previous: "前へ", next: "次へ", evidence: "根拠", offers: "オファー", details: "科学プロフィール", opening: "検証済みカタログを開いています…", failed: "公開カタログを開けませんでした。", retry: "再試行" },
});

function storedValue(key) {
  try { return localStorage.getItem(key); } catch { return null; }
}

function storeValue(key, value) {
  try { localStorage.setItem(key, value); } catch { /* Device preferences are optional. */ }
}

function preferredLocale() {
  const saved = storedValue("waajacu.locale");
  if (saved && SUPPORTED_LOCALES.has(saved)) return saved;
  for (const candidate of navigator.languages ?? [navigator.language]) {
    const primary = String(candidate).toLowerCase().split("-")[0];
    if (SUPPORTED_LOCALES.has(primary)) return primary;
  }
  return "en";
}

const preferences = { locale: preferredLocale() };

function t(key) {
  return TRANSLATIONS[preferences.locale]?.[key] ?? ENGLISH[key] ?? key;
}

function element(tagName, options = {}, children = []) {
  const node = document.createElement(tagName);
  if (options.className) node.className = options.className;
  if (options.text !== undefined && options.text !== null) node.textContent = String(options.text);
  if (options.id) node.id = options.id;
  for (const [name, value] of Object.entries(options.attrs ?? {})) {
    if (value !== undefined && value !== null && value !== false) {
      node.setAttribute(name, value === true ? "" : String(value));
    }
  }
  for (const child of Array.isArray(children) ? children : [children]) {
    if (child instanceof Node) node.append(child);
    else if (child !== undefined && child !== null) node.append(document.createTextNode(String(child)));
  }
  return node;
}

function paragraph(text, className) {
  return element("p", { text, className });
}

function routeLink(path, label, className, attrs = {}) {
  return element("a", {
    text: label,
    className,
    attrs: { href: routeHref(path), "data-route-link": "", ...attrs },
  });
}

function image(src, alt, className, width, height, loading = "lazy") {
  return element("img", {
    className,
    attrs: { src, alt, width, height, loading, decoding: "async" },
  });
}

function announce(message) {
  statusRegion.textContent = "";
  requestAnimationFrame(() => { statusRegion.textContent = message; });
}

function safeHttpUrl(value) {
  if (typeof value !== "string" || value.length > 2_048) return null;
  try {
    const url = new URL(value);
    return ["https:", "http:"].includes(url.protocol) && !url.username && !url.password ? url.href : null;
  } catch {
    return null;
  }
}

function externalLink(value, label, className = "source-link") {
  const href = safeHttpUrl(value);
  return href
    ? element("a", { text: label, className, attrs: { href, target: "_blank", rel: "noopener noreferrer" } })
    : element("span", { text: label, className: `${className} disabled` });
}

function humanLabel(value) {
  return String(value ?? "—").replaceAll("_", " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function formatDate(value) {
  const date = new Date(value);
  return Number.isFinite(date.valueOf())
    ? new Intl.DateTimeFormat(preferences.locale, { dateStyle: "medium" }).format(date)
    : String(value ?? "—");
}

function formatNumber(value) {
  return Number(value).toLocaleString(preferences.locale);
}

function qualityPercent(value) {
  const number = Number(value);
  return Number.isFinite(number) ? Math.max(0, Math.min(100, Math.round(number * 100))) : null;
}

function applyLocale() {
  document.documentElement.lang = preferences.locale;
  document.documentElement.dir = preferences.locale === "ar" ? "rtl" : "ltr";
  localeSelect.value = preferences.locale;
  localeSelect.setAttribute("aria-label", t("language"));
  const labels = { minerals: t("catalog").toUpperCase(), map: t("map").toUpperCase(), about: t("source").toUpperCase() };
  for (const link of document.querySelectorAll("[data-nav]")) link.textContent = labels[link.dataset.nav] ?? link.textContent;
  const footerLinks = document.querySelectorAll(".footer-nav a");
  if (footerLinks[0]) footerLinks[0].textContent = t("catalog");
  if (footerLinks[1]) footerLinks[1].textContent = t("map");
  if (footerLinks[2]) footerLinks[2].textContent = t("source");
  updateManifestBindings();
}

class CatalogClient {
  #worker;
  #nextId = 1;
  #pending = new Map();

  constructor() {
    this.#worker = new Worker(new URL(`./catalog-worker.js?v=${CATALOG_WORKER_REVISION}`, import.meta.url), {
      type: "module",
      name: "waajacu-catalog",
    });
    this.#worker.addEventListener("message", (event) => this.#onMessage(event.data));
    this.#worker.addEventListener("error", () => this.#failAll(new Error("The catalog worker stopped unexpectedly.")));
    this.#worker.addEventListener("messageerror", () => this.#failAll(new Error("The catalog worker returned an unreadable response.")));
  }

  #onMessage(raw) {
    let message;
    try {
      message = validateWorkerResponse(raw);
    } catch {
      const pending = Number.isSafeInteger(raw?.id) ? this.#pending.get(raw.id) : undefined;
      if (pending) {
        this.#pending.delete(raw.id);
        pending.removeAbort?.();
        pending.reject(new Error("The catalog worker returned an invalid protocol response."));
      }
      return;
    }
    const pending = this.#pending.get(message.id);
    if (!pending) return;
    this.#pending.delete(message.id);
    pending.removeAbort?.();
    if (message.type !== pending.type) {
      pending.reject(new Error("The catalog worker response did not match its request."));
    } else if (message.ok) {
      pending.resolve(message.result);
    } else {
      const error = new Error(message.error.message);
      error.code = message.error.code;
      pending.reject(error);
    }
  }

  #failAll(error) {
    for (const pending of this.#pending.values()) {
      pending.removeAbort?.();
      pending.reject(error);
    }
    this.#pending.clear();
  }

  request(type, payload, signal) {
    const id = this.#nextId++;
    const message = validateWorkerRequest({ id, type, payload });
    if (signal?.aborted) return Promise.reject(new DOMException("Request aborted", "AbortError"));
    return new Promise((resolve, reject) => {
      const abort = () => {
        this.#pending.delete(id);
        reject(new DOMException("Request aborted", "AbortError"));
      };
      if (signal) signal.addEventListener("abort", abort, { once: true });
      this.#pending.set(id, {
        type,
        resolve,
        reject,
        removeAbort: signal ? () => signal.removeEventListener("abort", abort) : undefined,
      });
      try {
        this.#worker.postMessage(message);
      } catch (error) {
        this.#pending.delete(id);
        if (signal) signal.removeEventListener("abort", abort);
        reject(error);
      }
    });
  }

  init() {
    return this.request("init", { manifestUrl: new URL("./catalog-manifest.json", import.meta.url).href });
  }
  search(input, signal) { return this.request("search", normalizeSearchParams(input), signal); }
  detail(slug, signal) { return this.request("detail", { slug }, signal); }
  evidence(slug, signal) { return this.request("evidence", { slug }, signal); }
  offers(slug, signal) { return this.request("offers", { slug }, signal); }
}

let catalogClient;
let catalogInit;
let manifest;
let manifestPromise;
let renderSequence = 0;
let routeController;
let mapLifecycle;
let hasRendered = false;
const webMcpLifetime = new AbortController();
let webMcpRegistration;

function catalog() {
  if (!catalogClient) catalogClient = new CatalogClient();
  return catalogClient;
}

async function ensureCatalog() {
  if (!catalogInit) {
    catalogInit = catalog().init().then((result) => {
      manifest = result.manifest;
      updateManifestBindings();
      return result;
    }).catch((error) => {
      catalogInit = undefined;
      throw error;
    });
  }
  return catalogInit;
}

function abortableWait(promise, signal) {
  if (!signal) return promise;
  if (signal.aborted) return Promise.reject(new DOMException("Request aborted", "AbortError"));
  return new Promise((resolve, reject) => {
    const abort = () => reject(new DOMException("Request aborted", "AbortError"));
    signal.addEventListener("abort", abort, { once: true });
    Promise.resolve(promise).then(resolve, reject).finally(() => {
      signal.removeEventListener("abort", abort);
    });
  });
}

async function searchMineralsForAgent(input, signal) {
  await abortableWait(ensureCatalog(), signal);
  if (signal?.aborted) throw new DOMException("Request aborted", "AbortError");
  const result = await catalog().search({
    query: input.query,
    page: input.page,
    pageSize: input.pageSize,
  }, signal);
  return { ...result, release_id: manifest.release_id };
}

async function getMineralForAgent(slug, signal) {
  await abortableWait(ensureCatalog(), signal);
  if (signal?.aborted) throw new DOMException("Request aborted", "AbortError");
  const mineral = await catalog().detail(slug, signal);
  if (!mineral) return { release_id: manifest.release_id, mineral: null, evidence: [] };
  const evidence = await catalog().evidence(slug, signal);
  return { release_id: manifest.release_id, mineral, evidence: evidence.items };
}

async function loadManifestSummary() {
  if (manifest) return manifest;
  if (!manifestPromise) {
    const url = new URL("./catalog-manifest.json", import.meta.url);
    manifestPromise = fetch(url, {
      cache: "no-store",
      credentials: "same-origin",
      redirect: "error",
      headers: { Accept: "application/json" },
    }).then(async (response) => {
      if (!response.ok) throw new Error(`Catalog manifest returned HTTP ${response.status}.`);
      const value = validateManifest(await response.json(), response.url);
      manifest = value;
      updateManifestBindings();
      return value;
    }).catch((error) => {
      manifestPromise = undefined;
      throw error;
    });
  }
  return manifestPromise;
}

function updateManifestBindings() {
  if (!manifest) return;
  const count = formatNumber(manifest.mineral_count);
  for (const node of document.querySelectorAll("[data-mineral-count]")) node.textContent = count;
  for (const node of document.querySelectorAll("[data-release-date]")) node.textContent = formatDate(manifest.generated_at);
  for (const node of document.querySelectorAll("[data-release-id]")) node.textContent = manifest.release_id;
  for (const node of document.querySelectorAll("[data-schema-version]")) node.textContent = `v${manifest.schema_version}`;
  for (const node of document.querySelectorAll("[data-database-sha]")) node.textContent = manifest.database.sha256;
  releaseSummary.textContent = `${count} minerals · ${formatDate(manifest.generated_at)}`;
}

function classificationCard({ path, index, title, copy, src, alt, className = "" }) {
  return element("a", {
    className: `classification-card ${className}`.trim(),
    attrs: { href: routeHref(path), "data-route-link": "", "aria-label": `${title}: ${copy}` },
  }, [
    element("span", { className: "classification-number", text: index }),
    element("div", { className: "classification-copy" }, [element("h2", { text: title }), paragraph(copy)]),
    image(src, alt, "classification-art", "2048", "809"),
    element("span", { className: "classification-arrow", text: "↗", attrs: { "aria-hidden": "true" } }),
  ]);
}

function sectionHeading(index, titleLines, copy, dark = false) {
  const titleChildren = Array.isArray(titleLines)
    ? [titleLines[0], element("span", { text: titleLines[1] })]
    : [titleLines];
  return element("header", { className: `section-heading-block${dark ? " heading-dark" : ""}` }, [
    element("div", {}, [
      paragraph(index, `kicker ${dark ? "kicker-gold" : "kicker-paper"}`),
      element("h2", {}, titleChildren),
    ]),
    paragraph(copy, "section-intro"),
  ]);
}

function renderHome() {
  const view = element("div", { className: "home-view" });
  const hero = element("section", { className: "atlas-hero", attrs: { "aria-labelledby": "home-title" } });
  const copy = element("div", { className: "hero-copy" }, [
    paragraph("OPEN DATA / EARTH SCIENCES", "kicker kicker-gold"),
    element("h1", { id: "home-title" }, ["Read the Earth.", element("em", { text: "Follow its structure." })]),
    paragraph("An open atlas for exploring minerals, formulas, crystal systems, and the places they are found.", "hero-intro"),
    element("div", { className: "hero-actions" }, [
      routeLink("/minerals", "EXPLORE THE CATALOG →", "button button-gold"),
      routeLink("/map", "OPEN THE WORLD MAP ⌖", "button button-outline"),
    ]),
  ]);
  const plate = element("figure", { className: "hero-plate" }, [
    image("./assets/atlas-quartz-v2.png", "Quartz crystal cluster surrounded by crystallographic diagrams, contour lines, and field annotations", "hero-quartz", "1589", "989", "eager"),
  ]);
  hero.append(
    element("div", { className: "hero-grid" }, [copy, plate]),
    element("div", { className: "hero-coordinate", attrs: { "aria-hidden": "true" } }, [
      element("span", { text: "SiO₂ / QUARTZ" }),
      element("span", { text: "TRIGONAL SYSTEM" }),
      element("span", { text: "EARTH / OPEN RECORD" }),
    ]),
  );

  const pathways = element("section", { className: "classification-grid", attrs: { "aria-label": "Explore the atlas" } }, [
    classificationCard({ path: "/minerals?view=systems", index: "01", title: "CRYSTAL SYSTEM", copy: "Seven geometric families describe how a crystal is ordered.", src: "./assets/atlas-crystal-system-v2.png", alt: "The seven crystal systems drawn as gold scientific diagrams", className: "classification-crystal" }),
    classificationCard({ path: "/minerals?view=families", index: "02", title: "CHEMICAL FAMILY", copy: "Composition connects minerals through shared elemental structures.", src: "./assets/atlas-chemical-family-v2.png", alt: "Chemical family diagram with molecular structure and diffraction graph", className: "classification-chemical" }),
    classificationCard({ path: "/map", index: "03", title: "PLACE OF ORIGIN", copy: "Locality keeps every specimen attached to a real landscape.", src: "./assets/atlas-place-origin-v2.png", alt: "World map and locality diagram for exploring mineral origins", className: "classification-origin" }),
  ]);

  const method = element("section", { className: "paper-section method-section", attrs: { "aria-labelledby": "method-title" } }, [
    image("./assets/atlas-mountain-v2.png", "", "mountain-watermark", "2173", "724"),
    sectionHeading("01 / FROM SPECIMEN TO KNOWLEDGE", ["Every stone", "carries a record."], "Careful observation, classification, locality documentation, and evidence review turn specimens into knowledge that anyone can inspect."),
    element("figure", { className: "method-plate" }, [
      image("./assets/atlas-method-v2.png", "Observe a mineral specimen, classify its composition and crystal structure, then verify the record with trusted sources", "method-art", "2128", "739"),
    ]),
  ]);

  const projectCatalog = element("a", { className: "project-card project-card-dark", attrs: { href: routeHref("/minerals"), "data-route-link": "", "aria-labelledby": "catalog-project-title" } }, [
    element("span", { className: "project-arrow", text: "↗", attrs: { "aria-hidden": "true" } }),
    paragraph("SEARCH / CLASSIFY / VERIFY", "kicker kicker-gold"),
    element("h3", { id: "catalog-project-title", text: "THE PUBLIC MINERAL CATALOG" }),
    paragraph("Search formulas, families, descriptions, identifiers, and evidence across the published collection."),
    image("./assets/atlas-crystal-system-v2.png", "Seven crystallographic systems", "project-art", "1942", "809"),
    element("div", { className: "project-meta" }, [
      element("strong", { text: "6,226", attrs: { "data-mineral-count": "" } }),
      element("span", { text: "PUBLISHED RECORDS" }),
    ]),
    element("span", { className: "project-action", text: "BROWSE MINERALS →" }),
  ]);

  const projectMap = element("a", { className: "project-card project-card-paper", attrs: { href: routeHref("/map"), "data-route-link": "", "aria-labelledby": "map-project-title" } }, [
    element("span", { className: "project-arrow", text: "↗", attrs: { "aria-hidden": "true" } }),
    paragraph("PLANETARY CONTEXT / INTERACTIVE VIEW", "kicker kicker-paper"),
    element("h3", { id: "map-project-title", text: "THE GEOLOGICAL WORLD MAP" }),
    paragraph("Open the existing environmental context map, preserved as a separate interactive instrument."),
    image("./assets/atlas-place-origin-v2.png", "World map and locality annotations", "project-art map-project-art", "2048", "768"),
    element("div", { className: "map-scope-note" }, [
      element("span", { text: "CURRENT LAYER" }),
      element("strong", { text: "FOREST / LAND / WATER CONTEXT" }),
    ]),
    element("span", { className: "project-action", text: "EXPLORE THE MAP →" }),
  ]);

  const projects = element("section", { className: "paper-section projects-section", attrs: { "aria-labelledby": "projects-title" } }, [
    sectionHeading("02 / EXPLORE THE COLLECTION", ["One mineral world.", "Many paths through it."], "Move from names and formulas to crystal families, evidence, and planetary context without leaving the public release."),
    element("div", { className: "project-grid" }, [projectCatalog, projectMap]),
  ]);

  const evidenceBand = element("section", { className: "evidence-band", attrs: { "aria-labelledby": "evidence-band-title" } }, [
    element("h2", { id: "evidence-band-title", text: "Claims should never outrun evidence." }),
    element("div", { className: "principle-grid" }, [
      element("article", {}, [element("span", { className: "principle-icon", text: "◎", attrs: { "aria-hidden": "true" } }), element("h3", { text: "PROVENANCE" }), paragraph("Every observation remains attached to its source, timestamp, and origin.")]),
      element("article", {}, [element("span", { className: "principle-icon", text: "◇", attrs: { "aria-hidden": "true" } }), element("h3", { text: "UNCERTAINTY" }), paragraph("The record says what is known, what is inferred, and why it matters.")]),
      element("article", {}, [element("span", { className: "principle-icon", text: "⌬", attrs: { "aria-hidden": "true" } }), element("h3", { text: "REPRODUCIBILITY" }), paragraph("Open methods and data let others verify and build on the work.")]),
    ]),
  ]);

  const source = element("section", { className: "source-section", attrs: { "aria-labelledby": "source-home-title" } }, [
    element("div", { className: "source-copy" }, [
      paragraph("03 / OPEN INFRASTRUCTURE", "kicker kicker-paper"),
      element("h2", { id: "source-home-title" }, ["Open by design.", element("em", { text: "Grounded in evidence." })]),
      paragraph("The catalog is a self-contained, read-only public release. Its database is checked locally before any record is searched."),
      routeLink("/about", "VIEW THE PUBLIC SOURCE →", "button button-ink"),
      element("dl", { className: "release-ledger" }, [
        element("div", {}, [element("dt", { text: "RECORDS" }), element("dd", { text: "6,226", attrs: { "data-mineral-count": "" } })]),
        element("div", {}, [element("dt", { text: "SCHEMA" }), element("dd", { text: "v1", attrs: { "data-schema-version": "" } })]),
        element("div", {}, [element("dt", { text: "MODE" }), element("dd", { text: "READ ONLY" })]),
      ]),
    ]),
    element("div", { className: "source-visual", attrs: { "aria-hidden": "true" } }, [
      image("./assets/atlas-source-v2.png", "", "source-art", "1920", "819"),
      element("pre", { text: "// verify\nsha256(database)\n  == manifest.digest\n\n// open\nmode: read_only\nsource: public_release" }),
    ]),
  ]);

  view.append(hero, pathways, method, projects, evidenceBand, source);
  updateManifestBindings();
  return view;
}

function loadingView(label = t("opening")) {
  return element("section", { className: "route-loading catalog-loading", attrs: { "aria-busy": "true" } }, [
    paragraph("PUBLIC CATALOG / LOCAL VERIFICATION", "kicker kicker-gold"),
    element("h1", { text: label }),
    paragraph("The database is authenticated locally before its records become searchable.", "loading-copy"),
    element("span", { className: "loading-rule", attrs: { "aria-hidden": "true" } }),
  ]);
}

function errorView(error, retry) {
  const section = element("section", { className: "route-error" }, [
    paragraph("CATALOG UNAVAILABLE", "kicker kicker-paper"),
    element("h1", { text: t("failed") }),
    paragraph(error instanceof Error ? error.message : "An unknown error occurred."),
  ]);
  const button = element("button", { text: t("retry"), className: "button button-ink", attrs: { type: "button" } });
  button.addEventListener("click", retry);
  section.append(button);
  return section;
}

function statusChip(value, className = "") {
  return element("span", { className: `status-chip ${className}`.trim(), text: humanLabel(value) });
}

function mineralCard(mineral) {
  const titleId = `mineral-${mineral.slug}`;
  const quality = qualityPercent(mineral.data_quality_score);
  return element("a", {
    className: "mineral-result",
    attrs: { href: routeHref(`/minerals/${encodeURIComponent(mineral.slug)}`), "data-route-link": "", "aria-labelledby": titleId },
  }, [
    element("span", { className: "result-index", text: mineral.public_id }),
    element("span", { className: "result-identity" }, [
      element("strong", { id: titleId, className: "result-name", text: mineral.canonical_name }),
      paragraph(mineral.description_excerpt || "No public description is available.", "result-description"),
    ]),
    element("span", { className: "result-science" }, [
      element("small", { text: mineral.mineral_family || "UNCLASSIFIED" }),
      element("bdi", { className: "result-formula", text: mineral.formula || "—" }),
    ]),
    element("span", { className: "result-trust" }, [
      statusChip(mineral.verification_status || "published", "verified"),
      element("span", { text: `${formatNumber(mineral.evidence_count)} source${mineral.evidence_count === 1 ? "" : "s"}` }),
      quality === null ? null : element("span", { text: `${quality}% quality` }),
    ].filter(Boolean)),
    element("span", { className: "result-arrow", text: "↗", attrs: { "aria-hidden": "true" } }),
  ]);
}

function searchForm(search) {
  const form = element("form", { className: "catalog-search", attrs: { role: "search" } });
  const label = element("label", { text: t("searchLabel"), attrs: { for: "catalog-search" } });
  const input = element("input", { id: "catalog-search", attrs: { type: "search", name: "q", value: search.query, placeholder: t("searchHint"), maxlength: "160", autocomplete: "off" } });
  const button = element("button", { text: `${t("search")} →`, className: "search-submit", attrs: { type: "submit" } });
  form.append(label, element("div", { className: "search-field" }, [input, button]));
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    const parameters = new URLSearchParams({ q: input.value, page: "1", page_size: String(search.pageSize) });
    navigate(`/minerals?${parameters}`);
  });
  return form;
}

function pager(search, result) {
  if (result.total_pages <= 1) return null;
  const params = (page) => new URLSearchParams({ q: search.query, page: String(page), page_size: String(search.pageSize) });
  const nav = element("nav", { className: "catalog-pager", attrs: { "aria-label": "Catalog pages" } });
  if (search.page > 1) nav.append(routeLink(`/minerals?${params(search.page - 1)}`, `← ${t("previous")}`, "pager-link"));
  nav.append(element("span", { className: "pager-position", text: `${Math.min(search.page, result.total_pages)} / ${result.total_pages}` }));
  if (search.page < result.total_pages) nav.append(routeLink(`/minerals?${params(search.page + 1)}`, `${t("next")} →`, "pager-link"));
  return nav;
}

async function renderCatalog(route, signal) {
  const result = await catalog().search(route.search, signal);
  const view = element("section", { className: "catalog-view" });
  const hero = element("header", { className: "catalog-hero", attrs: { "aria-labelledby": "catalog-title" } }, [
    element("div", { className: "catalog-hero-copy" }, [
      paragraph("PUBLIC CATALOG / VERIFIED RECORDS", "kicker kicker-gold"),
      element("h1", { id: "catalog-title", text: "Read structure through evidence." }),
      paragraph("Search names, formulas, mineral families, descriptions, and identifiers in the self-contained public release.", "route-intro"),
      element("dl", { className: "catalog-ledger" }, [
        element("div", {}, [element("dt", { text: "RECORDS" }), element("dd", { text: manifest ? formatNumber(manifest.mineral_count) : formatNumber(result.total), attrs: { "data-mineral-count": "" } })]),
        element("div", {}, [element("dt", { text: "MODE" }), element("dd", { text: "READ ONLY" })]),
        element("div", {}, [element("dt", { text: "VERIFICATION" }), element("dd", { text: "LOCAL SHA-256" })]),
      ]),
    ]),
    image("./assets/atlas-chemical-family-v2.png", "Chemical family and diffraction illustration", "catalog-hero-art", "1944", "809", "eager"),
  ]);
  const searchPanel = element("section", { className: "search-panel", attrs: { "aria-label": "Catalog search" } }, [
    searchForm(route.search),
    paragraph(route.search.query ? `${formatNumber(result.total)} records for “${route.search.query}”` : `${formatNumber(result.total)} published mineral records`, "search-summary"),
  ]);
  const results = element("section", { className: "catalog-results", attrs: { "aria-label": "Mineral records" } }, [
    element("div", { className: "results-labels", attrs: { "aria-hidden": "true" } }, [
      element("span", { text: "RECORD" }), element("span", { text: "MINERAL IDENTITY" }), element("span", { text: "FAMILY / FORMULA" }), element("span", { text: "VERIFICATION" }),
    ]),
    result.items.length ? element("div", { className: "result-list" }, result.items.map(mineralCard)) : element("div", { className: "empty-results" }, [element("h2", { text: "No matching mineral record." }), paragraph("Try a broader name, formula, family, or keyword.")]),
  ]);
  view.append(hero, searchPanel, results);
  const pagination = pager(route.search, result);
  if (pagination) view.append(pagination);
  return view;
}

function parsedJson(text) {
  if (typeof text !== "string" || text.length === 0) return null;
  try { return JSON.parse(text); } catch { return null; }
}

function structuredData(value, depth = 0) {
  if (depth > 4) return element("span", { text: "…" });
  if (value === null || typeof value !== "object") return element("span", { text: value === null ? "—" : String(value) });
  if (Array.isArray(value)) {
    const list = element("ul", { className: "structured-list" });
    for (const item of value.slice(0, 100)) list.append(element("li", {}, structuredData(item, depth + 1)));
    return list;
  }
  const list = element("dl", { className: "structured-data" });
  for (const [key, item] of Object.entries(value).slice(0, 100)) {
    list.append(element("div", {}, [element("dt", { text: humanLabel(key) }), element("dd", {}, structuredData(item, depth + 1))]));
  }
  return list;
}

function fact(label, value) {
  return element("div", { className: "record-fact" }, [element("dt", { text: label }), element("dd", { text: value ?? "—" })]);
}

function jsonPanel(title, raw) {
  const data = parsedJson(raw);
  if (data === null || (Array.isArray(data) && data.length === 0) || (typeof data === "object" && !Array.isArray(data) && Object.keys(data).length === 0)) return null;
  return element("section", { className: "record-panel" }, [paragraph("STRUCTURED DATA", "panel-index"), element("h2", { text: title }), structuredData(data)]);
}

function evidenceCard(item, index) {
  const title = item.title || item.work_title || `Evidence ${index + 1}`;
  const card = element("article", { className: "evidence-card" }, [
    element("div", { className: "evidence-meta" }, [
      element("span", { text: String(index + 1).padStart(2, "0") }),
      item.review_status ? statusChip(item.review_status, "verified") : null,
      item.license_spdx ? statusChip(item.license_spdx) : null,
    ].filter(Boolean)),
    element("h3", { text: title }),
    paragraph([item.publisher, item.attribution_party].filter(Boolean).join(" · ") || "Published source", "muted"),
  ]);
  const url = item.canonical_url || item.work_url;
  if (url) card.append(externalLink(url, "OPEN SOURCE ↗"));
  const claim = parsedJson(item.claim_json);
  if (claim !== null) card.append(element("details", { className: "claim-details" }, [element("summary", { text: item.claim_scope || "View attached claim" }), structuredData(claim)]));
  for (const notice of [item.changes_notice, item.no_endorsement_notice].filter(Boolean)) card.append(paragraph(notice, "notice"));
  return card;
}

function priceText(item) {
  const exponent = Number(item.currency_exponent);
  const minor = Number(item.price_minor);
  if (Number.isSafeInteger(minor) && Number.isInteger(exponent) && exponent >= 0 && exponent <= 6 && /^[A-Z]{3}$/.test(item.currency_code ?? "")) {
    try { return new Intl.NumberFormat(preferences.locale, { style: "currency", currency: item.currency_code }).format(minor / (10 ** exponent)); } catch { /* Exact fallback below. */ }
  }
  return item.price_minor === null ? "Price on request" : `${item.price_minor} ${item.currency_code ?? ""}`.trim();
}

function offerCard(item) {
  const card = element("article", { className: "offer-card" }, [
    element("div", { className: "offer-meta" }, [item.stock_status ? statusChip(item.stock_status) : null, item.verification_status ? statusChip(item.verification_status, "verified") : null].filter(Boolean)),
    element("h3", { text: item.title || item.provider_name || "Published offer" }),
    paragraph(item.provider_name || "Provider", "muted"),
    paragraph(priceText(item), "offer-price"),
  ]);
  const facts = [
    ["Basis", item.pricing_basis], ["Minimum", [item.minimum_order_quantity, item.minimum_order_unit].filter(Boolean).join(" ")],
    ["Purity", item.purity_text], ["Grade", item.grade], ["Origin", item.origin_country_code], ["Checked", item.last_checked_at ? formatDate(item.last_checked_at) : null],
  ].filter(([, value]) => value !== null && value !== undefined && value !== "");
  if (facts.length) card.append(element("dl", { className: "offer-facts" }, facts.map(([label, value]) => fact(label, value))));
  if (item.product_url) card.append(externalLink(item.product_url, "OPEN PROVIDER ↗"));
  return card;
}

async function renderMineral(route, signal) {
  const [mineral, evidenceResult, offerResult] = await Promise.all([
    catalog().detail(route.slug, signal), catalog().evidence(route.slug, signal), catalog().offers(route.slug, signal),
  ]);
  if (!mineral) return renderNotFound("That mineral is not part of this public release.");
  const offers = offerResult.items.filter((item) => isOfferActiveAt(item.expires_at));
  const quality = qualityPercent(mineral.data_quality_score);
  const isQuartz = mineral.slug === "quartz" || mineral.canonical_name.toLocaleLowerCase("en") === "quartz";
  const visual = isQuartz
    ? image("./assets/atlas-quartz-v2.png", "Quartz crystal cluster with crystallographic field annotations", "record-quartz", "1589", "989", "eager")
    : element("div", { className: "record-seal", attrs: { "aria-hidden": "true" } }, [
      element("span", { text: mineral.canonical_name.slice(0, 1).toUpperCase() }),
      element("bdi", { text: mineral.formula || "◇" }),
    ]);
  const hero = element("header", { className: "record-hero", attrs: { "aria-labelledby": "record-title" } }, [
    routeLink("/minerals", `← ${t("catalog").toUpperCase()}`, "record-back"),
    element("div", { className: "record-title" }, [
      paragraph(`MINERAL RECORD / ${mineral.public_id}`, "kicker kicker-gold"),
      element("h1", { id: "record-title", text: mineral.canonical_name }),
      element("div", { className: "record-status" }, [
        mineral.mineral_family ? statusChip(mineral.mineral_family) : null,
        mineral.nomenclature_status ? statusChip(mineral.nomenclature_status) : null,
        statusChip(mineral.verification_status || "published", "verified"),
      ].filter(Boolean)),
      paragraph(mineral.description || "No public description is available.", "record-description"),
      element("div", { className: "record-formula" }, [element("span", { text: "FORMULA" }), element("bdi", { text: mineral.formula || "—" })]),
    ]),
    element("div", { className: "record-visual" }, [visual]),
  ]);
  const trust = element("dl", { className: "record-trust", attrs: { "aria-label": "Record overview" } }, [
    fact("EVIDENCE", formatNumber(evidenceResult.items.length)),
    fact("ACTIVE OFFERS", formatNumber(offers.length)),
    fact("DATA QUALITY", quality === null ? "—" : `${quality}%`),
    fact("LICENSE", mineral.license_spdx || "—"),
  ]);
  const profileFacts = element("dl", { className: "record-facts" }, [
    fact("Public ID", mineral.public_id), fact("CAS number", mineral.cas_number), fact("Mineral family", mineral.mineral_family),
    fact("Discovery country", mineral.discovery_country), fact("Source kind", humanLabel(mineral.source_kind)), fact("Source status", humanLabel(mineral.source_status)),
    fact("Nomenclature", humanLabel(mineral.nomenclature_status)), fact("License", mineral.license_spdx),
  ]);
  const profile = element("section", { className: "record-panel profile-panel" }, [paragraph("01 / VERIFIED PROFILE", "panel-index"), element("h2", { text: t("details") }), profileFacts]);
  const dataPanels = [jsonPanel("Identifiers", mineral.identifiers_json), jsonPanel("Properties", mineral.properties_json), jsonPanel("Safety", mineral.safety_json)].filter(Boolean);
  if (mineral.first_reference || mineral.second_reference) {
    dataPanels.push(element("section", { className: "record-panel" }, [
      paragraph("PUBLISHED REFERENCES", "panel-index"), element("h2", { text: "References" }),
      element("ol", { className: "reference-list" }, [mineral.first_reference, mineral.second_reference].filter(Boolean).map((reference) => element("li", { text: reference }))),
    ]));
  }
  const evidence = element("section", { className: "record-disclosure", attrs: { "aria-labelledby": "record-evidence-title" } }, [
    sectionHeading("02 / SOURCES AND CLAIMS", [t("evidence"), "attached to the record."], "Licenses, attribution, review status, and claim data remain visible beside the mineral they support."),
    element("div", { className: "evidence-list" }, evidenceResult.items.length ? evidenceResult.items.map(evidenceCard) : [paragraph("No public evidence records are attached to this release.", "empty-results")]),
  ]);
  const offersSection = element("section", { className: "offers-section", attrs: { "aria-labelledby": "record-offers-title" } }, [
    element("header", { className: "offers-heading" }, [paragraph("03 / PUBLIC MARKET", "kicker kicker-gold"), element("h2", { id: "record-offers-title", text: t("offers") }), paragraph("Only active, published offers are shown. Provider pages remain the authoritative source.")]),
    offers.length ? element("div", { className: "offer-grid" }, offers.map(offerCard)) : paragraph("No unexpired public offers are available.", "empty-offers"),
  ]);
  const view = element("article", { className: "record-view" }, [
    hero,
    trust,
    element("div", { className: "record-layout" }, [profile, element("div", { className: "record-data-stack" }, dataPanels)]),
    evidence,
    offersSection,
  ]);
  return { node: view, title: `${mineral.canonical_name} · Waajacu’s Minerals` };
}

function renderMapShell() {
  const container = element("div", { id: "catalog-map-root", className: "map-container", attrs: { "aria-busy": "true" } }, [paragraph("Loading map…", "visually-hidden")]);
  const node = element("section", { className: "map-view", attrs: { "aria-labelledby": "map-title" } }, [
    element("header", { className: "map-hero" }, [
      element("div", { className: "map-hero-copy" }, [
        paragraph("MAP / PLANETARY CONTEXT", "kicker kicker-gold"),
        element("h1", { id: "map-title", text: "Place every record in a wider world." }),
        paragraph("The preserved map currently shows forest, land, and water context. Verified mineral occurrence points are not part of this release yet.", "route-intro"),
        element("div", { className: "scope-badge" }, [element("span", { text: "CURRENT SCOPE" }), element("strong", { text: "ENVIRONMENTAL CONTEXT / JRC 2020" })]),
      ]),
      image("./assets/atlas-place-origin-v2.png", "World map and locality illustration", "map-hero-art", "2048", "768", "eager"),
    ]),
    element("section", { className: "map-instrument", attrs: { "aria-labelledby": "map-instrument-title" } }, [
      element("header", { className: "map-instrument-heading" }, [paragraph("INTERACTIVE INSTRUMENT", "kicker kicker-paper"), element("h2", { id: "map-instrument-title", text: "World context layer" }), paragraph("Drag, inspect, and change the view inside the map frame. Leaving this page fully releases its input handlers.")]),
      container,
    ]),
  ]);
  return { node, container };
}

async function teardownMap() {
  if (!mapLifecycle) return;
  const lifecycle = mapLifecycle;
  mapLifecycle = undefined;
  lifecycle.controller.abort();
  if (typeof lifecycle.cleanup === "function") {
    try { await lifecycle.cleanup(); } catch (error) { console.warn("Map cleanup failed:", error); }
  }
}

async function mountMap(container, sequence) {
  const controller = new AbortController();
  mapLifecycle = { controller, cleanup: undefined };
  try {
    const configured = mapModuleMeta?.content?.trim();
    if (!configured) throw new Error("No map module is configured.");
    const moduleUrl = new URL(configured, import.meta.url);
    if (moduleUrl.origin !== location.origin || moduleUrl.username || moduleUrl.password || moduleUrl.hash) throw new Error("The map module URL must be same-origin.");
    const module = await import(moduleUrl.href);
    if (controller.signal.aborted || sequence !== renderSequence) return;
    if (typeof module.mountMineralsMap !== "function") throw new Error("The map module does not export mountMineralsMap().");
    container.replaceChildren();
    container.removeAttribute("aria-busy");
    const cleanup = await module.mountMineralsMap(container, { theme: "dark", signal: controller.signal });
    if (cleanup !== undefined && typeof cleanup !== "function") throw new Error("The map mount function returned an invalid cleanup value.");
    if (controller.signal.aborted || sequence !== renderSequence) {
      if (typeof cleanup === "function") await cleanup();
    } else {
      mapLifecycle.cleanup = cleanup;
    }
  } catch (error) {
    if (!controller.signal.aborted && sequence === renderSequence) {
      container.removeAttribute("aria-busy");
      container.replaceChildren(element("div", { className: "map-unavailable" }, [
        element("h3", { text: "The optional map is unavailable." }),
        paragraph("The mineral catalog remains fully usable without the map package."),
        routeLink("/minerals", "OPEN THE CATALOG →", "button button-ink"),
      ]));
      console.info("Optional map module unavailable:", error);
    }
  }
}

function renderAbout() {
  const releaseFacts = element("dl", { className: "source-facts" }, [
    fact("Format", manifest?.format ?? "waajacu-public-catalog-v1"),
    fact("Schema", manifest ? `v${manifest.schema_version}` : "v1"),
    fact("Generated", manifest ? formatDate(manifest.generated_at) : "Loading…"),
    fact("Minerals", manifest ? formatNumber(manifest.mineral_count) : "6,226"),
    fact("Release ID", manifest?.release_id ?? "Loading…"),
    fact("Database SHA-256", manifest?.database?.sha256 ?? "Loading…"),
  ]);
  const node = element("section", { className: "about-view", attrs: { "aria-labelledby": "about-title" } }, [
    element("header", { className: "about-hero" }, [
      element("div", { className: "about-hero-copy" }, [
        paragraph("SOURCE / OPEN INFRASTRUCTURE", "kicker kicker-gold"),
        element("h1", { id: "about-title" }, ["Open by design.", element("em", { text: "Grounded in evidence." })]),
        paragraph("Waajacu’s public catalog is a read-only projection of published mineral facts, evidence attribution, and public market offers.", "route-intro"),
      ]),
      image("./assets/atlas-source-v2.png", "", "about-source-art", "1920", "819", "eager"),
    ]),
    element("div", { className: "about-grid" }, [
      element("section", { className: "about-panel about-panel-dark" }, [
        paragraph("01 / LOCAL VERIFICATION", "kicker kicker-gold"),
        element("h2", { text: "Verified in your browser" }),
        paragraph("Every release names a content-addressed SQLite database. Its exact length and SHA-256 digest are checked before the database opens read-only with official SQLite WebAssembly."),
        paragraph("Operational accounts, review queues, and unpublished registry data never enter this standalone public application."),
        element("pre", { text: "fetch(manifest)\nverify(bytes, sha256)\nopen(read_only)\nquery(fixed_protocol)" }),
      ]),
      element("section", { className: "about-panel about-panel-paper" }, [
        paragraph("02 / CURRENT SNAPSHOT", "kicker kicker-paper"),
        element("h2", { text: "Current release" }),
        releaseFacts,
      ]),
    ]),
    element("section", { className: "about-principles" }, [
      paragraph("PROVENANCE / UNCERTAINTY / REPRODUCIBILITY", "kicker kicker-paper"),
      element("h2", { text: "Claims should never outrun evidence." }),
      paragraph("Source licenses, attribution, review status, and retrieval history stay attached to their records. Follow each evidence link for the authoritative source and its complete terms."),
    ]),
  ]);
  return node;
}

function renderNotFound(message = "This route does not exist.") {
  return element("section", { className: "route-error not-found" }, [
    paragraph("404 / LOST STRATUM", "kicker kicker-paper"),
    element("h1", { text: "This layer is not in the atlas." }),
    paragraph(message),
    routeLink("/", "RETURN TO THE HOMEPAGE →", "button button-ink"),
  ]);
}

function setActiveNavigation(route) {
  const active = route.name === "mineral" ? "minerals" : route.name;
  for (const link of document.querySelectorAll("[data-nav]")) {
    if (link.dataset.nav === active) link.setAttribute("aria-current", "page");
    else link.removeAttribute("aria-current");
  }
}

function navigate(destination) {
  const href = destination.startsWith("#/") ? destination : routeHref(destination);
  if (location.hash === href.slice(1)) renderCurrentRoute({ focus: true });
  else location.assign(href);
}

function finishRoute(node, title, { focus = true } = {}) {
  main.replaceChildren(node);
  document.title = title;
  updateManifestBindings();
  if (focus && hasRendered) main.focus({ preventScroll: true });
  hasRendered = true;
}

async function renderCurrentRoute({ focus = true } = {}) {
  const sequence = ++renderSequence;
  routeController?.abort();
  routeController = new AbortController();
  await teardownMap();
  if (sequence !== renderSequence) return;
  const route = parseRoute(location.href, APP_BASE_PATH);
  applyLocale();
  setActiveNavigation(route);
  header.classList.remove("menu-open");
  navToggle.setAttribute("aria-expanded", "false");

  try {
    if (route.name === "home") {
      finishRoute(renderHome(), "Waajacu’s Minerals — Read the Earth", { focus });
      void loadManifestSummary().catch(() => { /* The visual homepage remains independent of catalog startup. */ });
      announce("Mineral atlas ready");
      return;
    }
    if (route.name === "about") {
      finishRoute(renderAbout(), "Source · Waajacu’s Minerals", { focus });
      void loadManifestSummary().then(() => {
        if (sequence === renderSequence) finishRoute(renderAbout(), "Source · Waajacu’s Minerals", { focus: false });
      }).catch(() => { /* Release facts keep safe placeholders. */ });
      announce("Source page ready");
      return;
    }
    if (route.name === "map") {
      const map = renderMapShell();
      finishRoute(map.node, "Map · Waajacu’s Minerals", { focus });
      announce("Map page ready");
      await mountMap(map.container, sequence);
      return;
    }
    if (route.name === "not-found") {
      finishRoute(renderNotFound(), "Page not found · Waajacu’s Minerals", { focus });
      return;
    }

    main.replaceChildren(loadingView());
    await ensureCatalog();
    if (sequence !== renderSequence || routeController.signal.aborted) return;
    if (route.name === "minerals") {
      const node = await renderCatalog(route, routeController.signal);
      if (sequence !== renderSequence || routeController.signal.aborted) return;
      finishRoute(node, "Catalog · Waajacu’s Minerals", { focus });
      announce("Catalog ready");
      return;
    }
    if (route.name === "mineral") {
      const result = await renderMineral(route, routeController.signal);
      if (sequence !== renderSequence || routeController.signal.aborted) return;
      if (result?.node) finishRoute(result.node, result.title, { focus });
      else finishRoute(result, "Mineral record · Waajacu’s Minerals", { focus });
      announce("Mineral record ready");
      return;
    }
    finishRoute(renderNotFound(), "Page not found · Waajacu’s Minerals", { focus });
  } catch (error) {
    if (error?.name === "AbortError" || sequence !== renderSequence) return;
    finishRoute(errorView(error, () => renderCurrentRoute({ focus: false })), "Catalog unavailable · Waajacu’s Minerals", { focus });
    announce(t("failed"));
  }
}

navToggle.addEventListener("click", () => {
  const open = navToggle.getAttribute("aria-expanded") !== "true";
  navToggle.setAttribute("aria-expanded", String(open));
  header.classList.toggle("menu-open", open);
});

primaryNav.addEventListener("click", () => {
  navToggle.setAttribute("aria-expanded", "false");
  header.classList.remove("menu-open");
});

localeSelect.addEventListener("change", () => {
  if (!SUPPORTED_LOCALES.has(localeSelect.value)) return;
  preferences.locale = localeSelect.value;
  storeValue("waajacu.locale", preferences.locale);
  renderCurrentRoute({ focus: false });
});

document.querySelector(".skip-link").addEventListener("click", (event) => {
  event.preventDefault();
  main.focus({ preventScroll: true });
  main.scrollIntoView({ block: "start" });
});

addEventListener("hashchange", () => renderCurrentRoute({ focus: true }));
addEventListener("beforeunload", () => {
  routeController?.abort();
  void teardownMap();
});

addEventListener("pagehide", (event) => {
  if (event.persisted) return;
  webMcpLifetime.abort();
  webMcpRegistration?.dispose();
});

void registerMineralsWebMcp({
  modelContext: document.modelContext,
  signal: webMcpLifetime.signal,
  baseUrl: new URL(".", import.meta.url).href,
  searchMinerals: searchMineralsForAgent,
  getMineral: getMineralForAgent,
}).then((registration) => {
  webMcpRegistration = registration;
}).catch((error) => {
  if (!webMcpLifetime.signal.aborted) console.warn("WebMCP tools could not be registered.", error);
});

applyLocale();
renderCurrentRoute({ focus: false });
