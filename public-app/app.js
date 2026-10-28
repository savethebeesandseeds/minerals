import {
  codRelationshipLabel,
  compactStructuredValue,
  groupMineralObservations,
  isOfferActiveAt,
  normalizeSearchParams,
  parseRoute,
  formatFormula,
  plainCrystalSystem,
  publishedResearchProfile,
  researchedMineralFacts,
  summarizeMineralFact,
  sourceModelSummary,
  routeHref,
  validateManifest,
  validateWorkerRequest,
  validateWorkerResponse,
} from "./app-core.mjs?v=baa78f329bc1bb47685f5a62de6d53ea767b7ce9950bf0b0f25ef62c37f5f2aa";
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
  catalog: "Atlas", map: "Map", source: "Source", language: "Language",
  search: "Search", searchLabel: "Search minerals", searchHint: "Name, formula, family, or keyword",
  previous: "Previous", next: "Next", evidence: "Evidence", offers: "Offers", details: "Scientific profile",
  opening: "Opening the verified atlas…", failed: "The public atlas could not be opened.", retry: "Try again",
});
const TRANSLATIONS = Object.freeze({
  en: ENGLISH,
  es: { catalog: "Atlas", map: "Mapa", source: "Fuente", language: "Idioma", search: "Buscar", searchLabel: "Buscar minerales", searchHint: "Nombre, fórmula, familia o palabra clave", previous: "Anterior", next: "Siguiente", evidence: "Evidencia", offers: "Ofertas", details: "Perfil científico", opening: "Abriendo el atlas verificado…", failed: "No se pudo abrir el atlas público.", retry: "Reintentar" },
  de: { catalog: "Atlas", map: "Karte", source: "Quelle", language: "Sprache", search: "Suchen", searchLabel: "Minerale suchen", searchHint: "Name, Formel, Familie oder Stichwort", previous: "Zurück", next: "Weiter", evidence: "Nachweise", offers: "Angebote", details: "Wissenschaftliches Profil", opening: "Verifizierter Atlas wird geöffnet…", failed: "Der öffentliche Atlas konnte nicht geöffnet werden.", retry: "Erneut versuchen" },
  fr: { catalog: "Atlas", map: "Carte", source: "Source", language: "Langue", search: "Rechercher", searchLabel: "Rechercher des minéraux", searchHint: "Nom, formule, famille ou mot-clé", previous: "Précédent", next: "Suivant", evidence: "Sources", offers: "Offres", details: "Profil scientifique", opening: "Ouverture de l’atlas vérifié…", failed: "Impossible d’ouvrir l’atlas public.", retry: "Réessayer" },
  cs: { catalog: "Atlas", map: "Mapa", source: "Zdroj", language: "Jazyk", search: "Hledat", searchLabel: "Hledat minerály", searchHint: "Název, vzorec, skupina nebo klíčové slovo", previous: "Předchozí", next: "Další", evidence: "Zdroje", offers: "Nabídky", details: "Vědecký profil", opening: "Otevírání ověřeného atlasu…", failed: "Veřejný atlas se nepodařilo otevřít.", retry: "Zkusit znovu" },
  zh: { catalog: "图集", map: "地图", source: "来源", language: "语言", search: "搜索", searchLabel: "搜索矿物", searchHint: "名称、化学式、类别或关键词", previous: "上一页", next: "下一页", evidence: "证据", offers: "报价", details: "科学档案", opening: "正在打开已验证图集…", failed: "无法打开公共图集。", retry: "重试" },
  ar: { catalog: "الأطلس", map: "الخريطة", source: "المصدر", language: "اللغة", search: "بحث", searchLabel: "البحث عن المعادن", searchHint: "الاسم أو الصيغة أو العائلة أو كلمة مفتاحية", previous: "السابق", next: "التالي", evidence: "الأدلة", offers: "العروض", details: "الملف العلمي", opening: "جارٍ فتح الأطلس المتحقق منه…", failed: "تعذر فتح الأطلس العام.", retry: "إعادة المحاولة" },
  pt: { catalog: "Atlas", map: "Mapa", source: "Fonte", language: "Idioma", search: "Pesquisar", searchLabel: "Pesquisar minerais", searchHint: "Nome, fórmula, família ou palavra-chave", previous: "Anterior", next: "Seguinte", evidence: "Evidências", offers: "Ofertas", details: "Perfil científico", opening: "Abrindo o atlas verificado…", failed: "Não foi possível abrir o atlas público.", retry: "Tentar novamente" },
  hi: { catalog: "एटलस", map: "मानचित्र", source: "स्रोत", language: "भाषा", search: "खोजें", searchLabel: "खनिज खोजें", searchHint: "नाम, सूत्र, परिवार या मुख्य शब्द", previous: "पिछला", next: "अगला", evidence: "साक्ष्य", offers: "प्रस्ताव", details: "वैज्ञानिक प्रोफ़ाइल", opening: "सत्यापित एटलस खोला जा रहा है…", failed: "सार्वजनिक एटलस नहीं खोला जा सका।", retry: "फिर प्रयास करें" },
  ja: { catalog: "アトラス", map: "地図", source: "ソース", language: "言語", search: "検索", searchLabel: "鉱物を検索", searchHint: "名前、化学式、分類、キーワード", previous: "前へ", next: "次へ", evidence: "根拠", offers: "オファー", details: "科学プロフィール", opening: "検証済みアトラスを開いています…", failed: "公開アトラスを開けませんでした。", retry: "再試行" },
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

function githubIcon() {
  const namespace = "http://www.w3.org/2000/svg";
  const icon = document.createElementNS(namespace, "svg");
  icon.setAttribute("class", "github-mark");
  icon.setAttribute("viewBox", "0 0 16 16");
  icon.setAttribute("aria-hidden", "true");
  icon.setAttribute("focusable", "false");
  const path = document.createElementNS(namespace, "path");
  path.setAttribute("d", "M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.53-.49-.33-.84-.81-1.07-.81-1.07-.66-.45.05-.44.05-.44.73.05 1.11.75 1.11.75.65 1.11 1.69.79 2.1.6.07-.47.25-.79.46-.97-1.6-.18-3.29-.8-3.29-3.56 0-.79.28-1.43.74-1.93-.07-.18-.32-.91.07-1.9 0 0 .6-.19 1.98.74A6.9 6.9 0 0 1 8 3.89c.61 0 1.23.08 1.8.24 1.38-.93 1.98-.74 1.98-.74.39.99.14 1.72.07 1.9.46.5.74 1.14.74 1.93 0 2.77-1.69 3.38-3.3 3.56.26.22.49.66.49 1.34 0 .97-.01 1.75-.01 1.99 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8Z");
  icon.append(path);
  return icon;
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
  if (value === null || value === undefined || (typeof value === "string" && !value.trim())) return null;
  const number = Number(value);
  if (!Number.isFinite(number) || number <= 0) return null;
  return Math.max(0, Math.min(100, Math.round(number * 100)));
}

function formulaText(formula, className = "chemical-formula") {
  const node = element("bdi", { className });
  for (const part of formatFormula(formula)) {
    if (part.kind === "sub") node.append(element("sub", { text: part.text }));
    else if (part.kind === "sup") node.append(element("sup", { text: part.text }));
    else node.append(document.createTextNode(part.text));
  }
  return node;
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

function classificationCard({ path, title, src, alt, width, height, className = "" }) {
  return element("a", {
    className: `classification-card ${className}`.trim(),
    attrs: { href: routeHref(path), "data-route-link": "", "aria-label": title },
  }, [
    image(src, alt, "classification-art", width, height),
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
    paragraph("PUBLIC MINERAL ATLAS", "kicker kicker-gold"),
    element("h1", { id: "home-title" }, ["A public mineral", element("em", { text: "atlas." })]),
    paragraph("Find published mineral records by name, formula, properties, locality, evidence, and provenance. Where available, records also describe natural specimens, laboratory-grown counterparts, minerals reported in meteorites, and published offers.", "hero-intro"),
    element("div", { className: "hero-actions" }, [
      routeLink("/minerals", "OPEN THE ATLAS →", "button button-gold"),
      routeLink("/map", "VIEW THE MAP ⌖", "button button-outline"),
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
      element("span", { text: "EARTH / LABORATORY / METEORITE RECORDS" }),
    ]),
  );

  const pathways = element("section", { className: "classification-grid", attrs: { "aria-label": "Atlas entry points" } }, [
    classificationCard({ path: "/minerals?view=systems", title: "View minerals by crystal system", src: "./assets/atlas-crystal-system-v2.png", alt: "The seven crystal systems drawn as gold scientific diagrams", width: "1942", height: "809", className: "classification-crystal" }),
    classificationCard({ path: "/minerals?view=families", title: "View minerals by chemical family", src: "./assets/atlas-chemical-family-v2.png", alt: "Chemical family diagram with molecular structure and diffraction graph", width: "1944", height: "809", className: "classification-chemical" }),
    classificationCard({ path: "/map", title: "View the locality map", src: "./assets/atlas-place-origin-v2.png", alt: "World map and locality diagram", width: "2048", height: "768", className: "classification-origin" }),
  ]);

  const method = element("section", { className: "paper-section method-section", attrs: { "aria-labelledby": "method-title" } }, [
    image("./assets/atlas-mountain-v2.png", "", "mountain-watermark", "2173", "724"),
    sectionHeading("01 / MINERAL RECORDS", ["What the atlas", "records."], "A mineral record may include accepted names, formula, crystal system, chemical family, physical and optical properties, formation, locality, discovery, identifiers, safety, rarity, references, evidence, provenance, and published offers. Missing or uncertain information remains visible."),
    element("figure", { className: "method-plate" }, [
      image("./assets/atlas-method-v2.png", "Observe a mineral specimen, classify its composition and crystal structure, then verify the record with trusted sources", "method-art", "2128", "739"),
    ]),
  ]);

  const projectCatalog = element("a", { className: "project-card project-card-dark", attrs: { href: routeHref("/minerals"), "data-route-link": "", "aria-labelledby": "catalog-project-title" } }, [
    element("span", { className: "project-arrow", text: "↗", attrs: { "aria-hidden": "true" } }),
    paragraph("SEARCH / CLASSIFY / VERIFY", "kicker kicker-gold"),
    element("h3", { id: "catalog-project-title", text: "THE PUBLIC MINERAL ATLAS" }),
    paragraph("Search formulas, families, descriptions, identifiers, and evidence across the published collection."),
    image("./assets/atlas-crystal-system-v2.png", "Seven crystallographic systems", "project-art", "1942", "809"),
    element("div", { className: "project-meta" }, [
      element("strong", { text: "6,226", attrs: { "data-mineral-count": "" } }),
      element("span", { text: "PUBLISHED RECORDS" }),
    ]),
    element("span", { className: "project-action", text: "OPEN ATLAS →" }),
  ]);

  const projectMap = element("a", { className: "project-card project-card-paper", attrs: { href: routeHref("/map"), "data-route-link": "", "aria-labelledby": "map-project-title" } }, [
    element("span", { className: "project-arrow", text: "↗", attrs: { "aria-hidden": "true" } }),
    paragraph("ENVIRONMENTAL CONTEXT / INTERACTIVE VIEW", "kicker kicker-paper"),
    element("h3", { id: "map-project-title", text: "THE CONTEXT MAP" }),
    paragraph("View the existing forest, land, and water context map. Mineral occurrence points are not part of this release yet."),
    image("./assets/atlas-place-origin-v2.png", "World map and locality annotations", "project-art map-project-art", "2048", "768"),
    element("div", { className: "map-scope-note" }, [
      element("span", { text: "CURRENT LAYER" }),
      element("strong", { text: "FOREST / LAND / WATER CONTEXT" }),
    ]),
    element("span", { className: "project-action", text: "VIEW MAP →" }),
  ]);

  const projects = element("section", { className: "paper-section projects-section", attrs: { "aria-labelledby": "projects-title" } }, [
    sectionHeading("02 / ATLAS AND MAP", ["The atlas", "and map."], "Use the atlas to search published mineral records. Use the map to view the environmental context available in the current release."),
    element("div", { className: "project-grid" }, [projectCatalog, projectMap]),
  ]);

  const evidenceBand = element("section", { className: "evidence-band", attrs: { "aria-labelledby": "evidence-band-title" } }, [
    element("h2", { id: "evidence-band-title", text: "Precious stones and rare-earth minerals." }),
    element("div", { className: "principle-grid" }, [
      element("a", { className: "principle-path", attrs: { href: routeHref("/minerals"), "data-route-link": "" } }, [element("span", { className: "principle-icon", text: "◎", attrs: { "aria-hidden": "true" } }), element("h3", { text: "IDENTITY" }), paragraph("Search a name, formula, accepted synonym, or record identifier."), element("span", { className: "principle-action", text: "OPEN THE ATLAS →" })]),
      element("a", { className: "principle-path", attrs: { href: routeHref("/minerals"), "data-route-link": "" } }, [element("span", { className: "principle-icon", text: "◇", attrs: { "aria-hidden": "true" } }), element("h3", { text: "COMMERCE" }), paragraph("Published offers appear with mineral records when available. Provider pages remain authoritative for specimen claims, availability, and price."), element("span", { className: "principle-action", text: "OPEN THE ATLAS →" })]),
      element("a", { className: "principle-path", attrs: { href: routeHref("/map"), "data-route-link": "" } }, [element("span", { className: "principle-icon", text: "⌖", attrs: { "aria-hidden": "true" } }), element("h3", { text: "PLACE" }), paragraph("Read documented localities in mineral records; use the map for the environmental layers currently available."), element("span", { className: "principle-action", text: "OPEN THE MAP →" })]),
    ]),
  ]);

  const protocolLink = element("a", {
    className: "button button-ink protocol-button",
    attrs: {
      href: "https://github.com/savethebeesandseeds/minerals",
      target: "_blank",
      rel: "noopener noreferrer",
      "aria-label": "View the open source protocol on GitHub (opens in a new tab)",
    },
  }, [githubIcon(), element("span", { text: "VIEW THE OPEN SOURCE PROTOCOL" }), element("span", { text: "→", attrs: { "aria-hidden": "true" } })]);

  const source = element("section", { className: "source-section", attrs: { "aria-labelledby": "source-home-title" } }, [
    element("div", { className: "source-copy" }, [
      paragraph("03 / PROTOCOL IN DESIGN", "kicker kicker-paper"),
      element("h2", { id: "source-home-title" }, ["Proposed safeguards", element("em", { text: "for mineral transactions." })]),
      paragraph("A future protocol is being designed to support selective disclosure, signed provenance claims, and checks for conflict-related sourcing risk. The current release contains the public atlas and published offer records only; it does not yet provide private transactions, cryptographic verification, or sourcing guarantees."),
      protocolLink,
      element("dl", { className: "release-ledger" }, [
        element("div", {}, [element("dt", { text: "PROTOCOL" }), element("dd", { text: "IN DESIGN" })]),
        element("div", {}, [element("dt", { text: "TRANSACTIONS" }), element("dd", { text: "NOT IMPLEMENTED" })]),
        element("div", {}, [element("dt", { text: "SOURCING" }), element("dd", { text: "NO GUARANTEE" })]),
      ]),
    ]),
    element("div", { className: "source-visual", attrs: { "aria-hidden": "true" } }, [
      image("./assets/atlas-source-v2.png", "", "source-art", "1920", "819"),
      element("pre", { text: "// proposal — not implemented\nprovenance := signed_claim\nprivacy := selective_disclosure\nsourcing := risk_attestation" }),
    ]),
  ]);

  view.append(hero, pathways, method, projects, evidenceBand, source);
  updateManifestBindings();
  return view;
}

function loadingView(label = t("opening")) {
  return element("section", { className: "route-loading catalog-loading", attrs: { "aria-busy": "true" } }, [
    paragraph("PUBLIC ATLAS / LOCAL VERIFICATION", "kicker kicker-gold"),
    element("h1", { text: label }),
    paragraph("The database is authenticated locally before its records become searchable.", "loading-copy"),
    element("span", { className: "loading-rule", attrs: { "aria-hidden": "true" } }),
  ]);
}

function errorView(error, retry) {
  const section = element("section", { className: "route-error" }, [
    paragraph("ATLAS UNAVAILABLE", "kicker kicker-paper"),
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
      mineral.formula ? formulaText(mineral.formula, "result-formula chemical-formula") : element("bdi", { className: "result-formula chemical-formula", text: "—" }),
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
  const nav = element("nav", { className: "catalog-pager", attrs: { "aria-label": "Atlas pages" } });
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
      paragraph("PUBLIC ATLAS / READ-ONLY RELEASE", "kicker kicker-gold"),
      element("h1", { id: "catalog-title", text: "Public mineral records." }),
      paragraph("Search names, formulas, mineral families, descriptions, and identifiers in the self-contained public release.", "route-intro"),
      element("dl", { className: "catalog-ledger" }, [
        element("div", {}, [element("dt", { text: "RECORDS" }), element("dd", { text: manifest ? formatNumber(manifest.mineral_count) : formatNumber(result.total), attrs: { "data-mineral-count": "" } })]),
        element("div", {}, [element("dt", { text: "MODE" }), element("dd", { text: "READ ONLY" })]),
        element("div", {}, [element("dt", { text: "VERIFICATION" }), element("dd", { text: "LOCAL SHA-256" })]),
      ]),
    ]),
    image("./assets/atlas-chemical-family-v2.png", "Chemical family and diffraction illustration", "catalog-hero-art", "1944", "809", "eager"),
  ]);
  const searchPanel = element("section", { className: "search-panel", attrs: { "aria-label": "Atlas search" } }, [
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

function lazyDetails(label, render) {
  const details = element("details", { className: "claim-details" }, [element("summary", { text: label })]);
  let loaded = false;
  details.addEventListener("toggle", () => {
    if (details.open && !loaded) {
      details.append(render());
      loaded = true;
    }
  });
  return details;
}

function progressiveCollection(items, render, { tag = "div", className = "", pageSize = 20, registerReveal = null } = {}) {
  const list = element(tag, { className });
  const wrapper = element("div", {}, [list]);
  let shown = 0;
  const more = element("button", { className: "pager-link", attrs: { type: "button" } });
  const appendUntil = (count) => {
    const end = Math.min(count, items.length);
    for (; shown < end; shown += 1) list.append(render(items[shown], shown));
    const remaining = items.length - shown;
    more.textContent = `Show ${Math.min(pageSize, remaining)} more (${formatNumber(remaining)} remaining)`;
    if (remaining === 0) more.remove();
  };
  const appendNext = () => appendUntil(shown + pageSize);
  registerReveal?.((index) => appendUntil(Math.max(shown, Math.ceil((index + 1) / pageSize) * pageSize)));
  more.addEventListener("click", appendNext);
  wrapper.append(more);
  appendNext();
  return wrapper;
}

function structuredData(value, depth = 0, allowCompact = true) {
  if (value === null || typeof value !== "object") return element("span", { text: value === null ? "—" : String(value) });
  if (depth > 4) {
    const size = Array.isArray(value) ? value.length : Object.keys(value).length;
    return lazyDetails(`Expand ${formatNumber(size)} ${Array.isArray(value) ? "items" : "fields"}`, () => structuredData(value, 0, allowCompact));
  }
  const compact = allowCompact ? compactStructuredValue(value) : null;
  if (compact) {
    return element("div", { className: "structured-value" }, [
      element("span", { className: "structured-value-summary", text: compact.text }),
      Object.keys(compact.remaining).length ? structuredData(compact.remaining, depth + 1, false) : null,
    ]);
  }
  if (Array.isArray(value)) {
    return progressiveCollection(value, (item) => element("li", {}, structuredData(item, depth + 1)), { tag: "ul", className: "structured-list" });
  }
  return progressiveCollection(Object.entries(value), ([key, item]) => element("div", {}, [element("dt", { text: humanLabel(key) }), element("dd", {}, structuredData(item, depth + 1))]), { tag: "dl", className: "structured-data" });
}

function fact(label, value) {
  return element("div", { className: "record-fact" }, [element("dt", { text: label }), element("dd", { text: value ?? "—" })]);
}

function researchSource(item) {
  const source = element("div", { className: "research-source" });
  if (item.source_url) source.append(externalLink(item.source_url, "SOURCE ↗"));
  if (item.locator) source.append(paragraph(item.locator, "muted"));
  const attribution = [item.source_title, item.publisher, item.license_spdx].filter(Boolean);
  if (attribution.length) source.append(paragraph(attribution.join(" · "), "muted"));
  if (item.bibliography && typeof item.bibliography === "object") {
    const bibliography = item.bibliography;
    const authors = Array.isArray(bibliography.authors) ? bibliography.authors.filter((author) => typeof author === "string").join("; ") : bibliography.authors;
    const citation = [typeof authors === "string" ? authors : null, bibliography.year, bibliography.journal].filter((value) => value !== null && value !== undefined && value !== "");
    if (citation.length) source.append(paragraph(citation.join(" · "), "muted"));
  }
  if (item.attribution && typeof item.attribution === "object") {
    source.append(lazyDetails("Source credits and reuse", () => structuredData(item.attribution)));
  }
  return source.childNodes.length ? source : null;
}

function researchQualification(item) {
  const notes = [item.qualification, item.scientific_note, item.qualifiers].filter((value) => value !== null && value !== undefined && value !== "" && (typeof value !== "object" || Object.keys(value).length));
  return notes.map((value) => typeof value === "string" ? paragraph(value, "notice") : element("div", { className: "notice" }, structuredData(value)));
}

function codReference(item) {
  return element("article", { className: "research-item" }, [
    element("h3", { text: `COD ${item.cod_id}${item.revision ? ` · revision ${item.revision}` : ""}` }),
    item.status ? paragraph(codRelationshipLabel(item.status), "muted") : null,
    item.relationship ? paragraph(humanLabel(item.relationship)) : null,
    item.primary_assignment === true ? paragraph("Primary COD association", "notice") : null,
    researchSource(item), ...researchQualification(item),
  ].filter(Boolean));
}

function crystalModel(item, references) {
  const summary = sourceModelSummary(item);
  const associations = Array.isArray(item.source_associations) ? item.source_associations : [item];
  const relationshipNotes = [...new Set(associations.flatMap((association) => [
    association.status ? codRelationshipLabel(association.status) : null,
    association.relationship ? humanLabel(association.relationship) : null,
  ]).filter(Boolean))];
  return element("article", { className: "research-item source-model" }, [
    element("h3", {}, [element("span", { text: `Source model · COD ${item.cod_id}${item.revision ? ` · revision ${item.revision}` : ""}` }), references?.citation(item)].filter(Boolean)),
    relationshipNotes.length ? paragraph(relationshipNotes.join(" · "), "muted") : null,
    element("dl", { className: "source-model-facts" }, summary.rows.map(({ label, value }) => fact(label, value))),
    summary.cell.length ? element("dl", { className: "source-model-cell", attrs: { "aria-label": "Deposited unit cell" } }, summary.cell.map(({ label, value }) => fact(label, value))) : null,
    ...summary.qualification.filter((note) => !(relationshipNotes.length && note === "Deposited source model; relationship to this mineral retains the source classifications."))
      .map((note) => typeof note === "string" ? paragraph(note, "notice") : element("div", { className: "notice" }, structuredData(note))),
    lazyDetails("Model measurements and source provenance", () => structuredData(summary.record)),
  ].filter(Boolean));
}

function evidenceCard(item, index) {
  const title = item.title || item.work_title || `Evidence ${index + 1}`;
  const card = element("article", { className: "evidence-card" }, [
    element("div", { className: "evidence-meta" }, [
      element("span", { text: String(index + 1).padStart(2, "0") }),
      item.review_status ? statusChip(item.review_status, item.review_status === "verified" ? "verified" : "") : null,
      item.license_spdx ? statusChip(item.license_spdx) : null,
    ].filter(Boolean)),
    element("h3", { text: title }),
    item.publisher || item.attribution_party ? paragraph([item.publisher, item.attribution_party].filter(Boolean).join(" · "), "muted") : null,
  ]);
  const url = item.canonical_url || item.work_url;
  if (url) card.append(externalLink(url, "OPEN SOURCE ↗"));
  const claim = parsedJson(item.claim_json);
  if (claim !== null) card.append(lazyDetails(item.claim_scope || "View attached claim", () => structuredData(claim)));
  card.append(lazyDetails("Source provenance", () => structuredData({ confidence: item.confidence, retrieved_at: item.retrieved_at, content_hash: item.content_hash, license_url: item.license_url, work_title: item.work_title, derived_output_license_spdx: item.derived_output_license_spdx })));
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

function articleJump(target, label, className = "article-jump", beforeJump = null) {
  const button = element("button", { text: label, className, attrs: { type: "button", "aria-controls": target } });
  button.addEventListener("click", () => {
    beforeJump?.();
    const section = document.getElementById(target);
    if (!section) return;
    section.scrollIntoView({ block: "start", behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "instant" : "smooth" });
    section.focus({ preventScroll: true });
  });
  return button;
}

function articleReferences(research, evidence) {
  const entries = [];
  const descriptionItems = [];
  const byUrl = new Map();
  const sourceFor = (item) => {
    const urls = [...new Set([item.source_url, item.canonical_url, item.work_url].map(safeHttpUrl).filter(Boolean))];
    const url = urls[0] || null;
    const codHosts = new Set(["crystallography.net", "www.crystallography.net"]);
    const pinnedCod = (candidate) => {
      if (!candidate) return null;
      const parsed = new URL(candidate);
      return codHosts.has(parsed.hostname) && !parsed.search && !parsed.hash ? parsed.pathname.match(/^\/cod\/(\d{7})\.cif@(\d+)$/) : null;
    };
    const pinned = pinnedCod(url);
    if (pinned) return { url, urls, key: `cod:${pinned[1]}@${pinned[2]}` };
    if (url) {
      const parsed = new URL(url);
      const page = codHosts.has(parsed.hostname) && !parsed.search && !parsed.hash ? parsed.pathname.match(/^\/cod\/(\d{7})\.html$/) : null;
      const work = pinnedCod(safeHttpUrl(item.work_url));
      if (page && work && page[1] === work[1]) return { url, urls, key: `cod:${work[1]}@${work[2]}` };
      if (["doi.org", "dx.doi.org"].includes(parsed.hostname) && !parsed.search && !parsed.hash) {
        try {
          const doi = decodeURIComponent(parsed.pathname.slice(1));
          if (/^10\.\d{4,9}\/\S+$/i.test(doi)) return { url, urls, key: `doi:${doi.toLowerCase()}` };
        } catch { /* Unrecognized encodings retain their exact source URL. */ }
      }
    }
    return { url, urls, key: url || item };
  };
  const add = (item, kind = "research") => {
    if (!item || typeof item !== "object" || Array.isArray(item)) return null;
    const { url, urls, key } = sourceFor(item);
    const title = [item.source_title, item.title, item.work_title].find((value) => typeof value === "string" && value.trim());
    let entry = byUrl.get(key);
    if (!entry) {
      entry = { number: entries.length + 1, url, urls: [], items: [], title: title || "Published source" };
      entries.push(entry);
      byUrl.set(key, entry);
    }
    if (entry.title === "Published source" && title) entry.title = title;
    for (const candidate of urls) if (!entry.urls.includes(candidate)) entry.urls.push(candidate);
    if (!entry.items.some((saved) => saved.item === item)) entry.items.push({ item, kind });
    return entry;
  };
  const description = research?.description;
  for (const url of description?.source_urls || []) {
    const metadata = Array.isArray(description.references) ? description.references.find((item) => item?.source_url === url) : null;
    const item = { ...description, ...metadata, source_url: url, license_spdx: metadata?.license_spdx || description.license_spdx, license_scope: description.license_scope ?? metadata?.license_scope };
    descriptionItems.push(item);
    add(item);
  }
  for (const item of [...(research?.observations || []), ...(research?.cod_records || []), ...(research?.structures || [])]) add(item);
  for (const item of evidence) add(item, "evidence");
  const citation = (item) => {
    if (!item || typeof item !== "object" || Array.isArray(item)) return null;
    const entry = byUrl.get(sourceFor(item).key);
    if (!entry) return null;
    const button = articleJump(`mineral-reference-${entry.number}`, `[${entry.number}]`, "article-citation", () => references.reveal?.(entry.number));
    button.setAttribute("aria-label", `Go to reference ${entry.number}: ${entry.title}`);
    return button;
  };
  const references = { entries, citation, descriptionItems, reveal: null };
  return references;
}

function referenceCollection(references) {
  const list = element("ol", { className: "article-references" });
  const wrapper = element("div", {}, [list]);
  const more = element("button", { className: "pager-link", attrs: { type: "button" } });
  let shown = 0;
  const appendUntil = (count) => {
    const end = Math.min(count, references.entries.length);
    for (; shown < end; shown += 1) list.append(referenceEntry(references.entries[shown]));
    const remaining = references.entries.length - shown;
    more.textContent = `Show ${Math.min(20, remaining)} more references (${formatNumber(remaining)} remaining)`;
    if (!remaining) more.remove();
  };
  references.reveal = (number) => appendUntil(Math.max(shown, Math.ceil(number / 20) * 20));
  more.addEventListener("click", () => appendUntil(shown + 20));
  wrapper.append(more);
  appendUntil(20);
  return wrapper;
}

function observationContent(observation) {
  const hasDisplayValue = ({ value }) => value !== null && value !== undefined
    && (typeof value !== "string" || value.trim())
    && (typeof value !== "object" || Object.keys(value).length);
  const context = observation.context.filter(hasDisplayValue);
  const inlineContext = context.filter(({ value }) => typeof value !== "object");
  const structuredContext = context.filter(({ value }) => typeof value === "object");
  const scopeLabels = new Set(["Applies to", "Source scope", "Production kind"]);
  return [
      observation.hasValue ? structuredData(observation.value) : element("span", { text: "—" }),
      inlineContext.length ? paragraph(inlineContext.map(({ label, value }) => `${label}: ${scopeLabels.has(label) ? String(value).replaceAll("_", " ") : String(value)}`).join(" · "), "observation-context") : null,
      ...structuredContext.map(({ label, value }) => element("div", { className: "observation-context" }, [paragraph(label), structuredData(value)])),
      ...observation.qualifications.filter(hasDisplayValue).map(({ value }) => typeof value === "string" ? paragraph(value, "notice") : element("div", { className: "notice" }, structuredData(value))),
      Object.keys(observation.extraFields).length ? lazyDetails("Additional reported fields", () => structuredData(observation.extraFields)) : null,
  ];
}

function compactObservation(observation, references, id) {
  return element("div", { id, className: "article-observation", attrs: { tabindex: "-1" } }, [
    element("dt", {}, [element("span", { text: observation.label }), references.citation(observation.raw)]),
    element("dd", {}, observationContent(observation)),
  ]);
}

function infoboxProperty(group, references, reportTargets) {
  const summary = summarizeMineralFact(group);
  const targets = summary.reports.map(({ raw }) => reportTargets.get(raw)).filter(Boolean);
  const target = targets[0]?.id || (group.general.length ? "mineral-properties" : null);
  const value = target ? articleJump(target, summary.text, "infobox-summary-link", () => {
    for (const entry of targets) entry.reveal();
  }) : element("span", { text: summary.text });
  if (target) value.setAttribute("aria-label", `Read full ${group.label.toLowerCase()} ${summary.reports.length > 1 ? "reports" : "report"}: ${summary.text}`);
  const citations = new Map();
  for (const { raw } of summary.reports) {
    const citation = references.citation(raw);
    if (citation) citations.set(citation.getAttribute("aria-controls"), citation);
  }
  return element("div", { className: "record-fact infobox-property" }, [
    element("dt", { text: group.label }),
    element("dd", { className: "infobox-property-value" }, [
      value,
      summary.qualifiers.length ? element("span", { className: "infobox-summary-qualifiers", text: ` (${summary.qualifiers.join("; ")})` }) : null,
      ...citations.values(),
    ]),
  ]);
}

function referenceEntry(entry) {
  const first = entry.items[0].item;
  const isText = (value) => typeof value === "string" && value.trim();
  const credits = [...new Set(entry.items.flatMap(({ item }) => [item.publisher, item.attribution_party]).filter(isText))];
  const sourceLicenses = [...new Set(entry.items.map(({ item }) => item.source_license_spdx).filter(isText))];
  const contentLicenses = [...new Set(entry.items.map(({ item }) => item.license_spdx).filter(isText))];
  const titleLink = externalLink(entry.url, entry.title, "reference-title");
  titleLink.setAttribute("title", entry.title);
  return element("li", { id: `mineral-reference-${entry.number}`, className: "article-reference", attrs: { tabindex: "-1" } }, [
    element("div", { className: "reference-summary" }, [
      titleLink,
      lazyDetails("Details", () => element("div", { className: "reference-provenance" }, [
        paragraph(entry.title, "reference-full-title"),
        credits.length ? paragraph(credits.join(" · "), "reference-credits") : null,
        sourceLicenses.length ? paragraph(`Source license: ${sourceLicenses.join(" · ")}`, "reference-credits") : null,
        contentLicenses.length ? paragraph(`Attached content license: ${contentLicenses.join(" · ")}`, "reference-credits") : null,
        isText(first.locator) ? paragraph(first.locator, "reference-credits") : null,
        element("div", { className: "reference-links" }, entry.urls.map((url, index) => externalLink(url, `Source link ${index + 1} ↗`))),
        paragraph(`${formatNumber(entry.items.length)} supporting ${entry.items.length === 1 ? "entry" : "entries"}`, "reference-credits"),
        progressiveCollection(entry.items, ({ item, kind }, index) => kind === "evidence" ? evidenceCard(item, index) : structuredData(item), { pageSize: 8 }),
      ])),
    ]),
  ]);
}

async function renderMineral(route, signal) {
  const [mineral, evidenceResult, offerResult] = await Promise.all([
    catalog().detail(route.slug, signal), catalog().evidence(route.slug, signal), catalog().offers(route.slug, signal),
  ]);
  if (!mineral) return renderNotFound("That mineral is not part of this public release.");
  const offers = offerResult.items.filter((item) => isOfferActiveAt(item.expires_at));
  const quality = qualityPercent(mineral.data_quality_score);
  const { properties, research } = publishedResearchProfile(mineral.properties_json);
  const references = articleReferences(research, evidenceResult.items);
  const researchedFacts = researchedMineralFacts(properties, research);
  const crystalSystem = plainCrystalSystem(researchedFacts);
  const summaryFacts = crystalSystem ? researchedFacts.filter((group) => group.key !== "crystal_system") : researchedFacts;
  const reportTargets = new Map();
  const sections = [];
  const addSection = (id, title, children) => {
    const headingId = `mineral-${id}`;
    const node = element("section", { className: "article-section", attrs: { "aria-labelledby": headingId } }, [
      element("h2", { id: headingId, text: title, attrs: { tabindex: "-1" } }), ...children.filter(Boolean),
    ]);
    sections.push({ id: headingId, title, node });
  };
  if (Object.keys(properties).length) addSection("properties", "Properties", [structuredData(properties)]);
  for (const { key, label, observations } of groupMineralObservations(research?.observations)) {
    let reveal = () => {};
    for (const [index, observation] of observations.entries()) {
      reportTargets.set(observation.raw, { id: `mineral-report-${key}-${index + 1}`, reveal: () => reveal(index) });
    }
    addSection(`observations-${key}`, label, [
      progressiveCollection(observations, (item) => compactObservation(item, references, reportTargets.get(item.raw).id), {
        tag: "dl", className: "article-observations", pageSize: 12, registerReveal: (callback) => { reveal = callback; },
      }),
    ]);
  }
  if (research?.structures.length || research?.cod_records.length) {
    const disclosures = [];
    if (research.structures.length) disclosures.push(progressiveCollection(research.structures, (item) => crystalModel(item, references), { pageSize: 6 }));
    if (research.cod_records.length) disclosures.push(lazyDetails(`COD source relationships (${formatNumber(research.cod_records.length)})`, () => progressiveCollection(research.cod_records, codReference, { pageSize: 12 })));
    addSection("models", research.structures.length ? "Crystal models" : "Crystallographic sources", disclosures);
  }
  const safety = parsedJson(mineral.safety_json);
  if (safety && Object.keys(safety).length) addSection("safety", "Safety", [structuredData(safety)]);
  const identifiers = parsedJson(mineral.identifiers_json);
  if (identifiers && Object.keys(identifiers).length) addSection("identifiers", "Identifiers", [structuredData(identifiers)]);
  if (references.entries.length || mineral.first_reference || mineral.second_reference) addSection("references", "References", [
    references.entries.length ? referenceCollection(references) : null,
    mineral.first_reference || mineral.second_reference ? lazyDetails("Registry bibliography", () => element("ul", { className: "reference-list" }, [mineral.first_reference, mineral.second_reference].filter(Boolean).map((reference) => element("li", { text: reference })))) : null,
  ]);
  if (offers.length) addSection("offers", `Published offers (${formatNumber(offers.length)})`, [element("div", { className: "offer-grid" }, offers.map(offerCard))]);
  const description = [mineral.description, research?.description?.text].find((value) => typeof value === "string" && value.trim());
  const introduction = description ? element("section", { className: "article-introduction", attrs: { "aria-labelledby": "mineral-overview" } }, [
    element("h2", { id: "mineral-overview", className: "visually-hidden", text: "Overview", attrs: { tabindex: "-1" } }),
    paragraph(description, "article-lead"),
    research?.description?.source_urls.length ? element("div", { className: "description-citations" }, [element("span", { text: "Description sources" }), ...references.descriptionItems.map(references.citation)]) : null,
  ]) : null;
  const infobox = element("aside", { className: "mineral-infobox", attrs: { "aria-label": `${mineral.canonical_name} facts` } }, [
    mineral.formula?.trim() ? element("div", { className: "infobox-formula" }, [element("span", { text: "Chemical formula" }), formulaText(mineral.formula)]) : null,
    element("dl", { className: "infobox-facts" }, [
      ...[["Crystal system", crystalSystem], ["Mineral family", mineral.mineral_family], ["Nomenclature", mineral.nomenclature_status ? humanLabel(mineral.nomenclature_status) : null], ["Discovery country", mineral.discovery_country], ["CAS number", mineral.cas_number]].filter(([, value]) => value !== undefined && value !== null && value !== "").map(([label, value]) => fact(label, value)),
    ]),
    summaryFacts.length ? element("section", { className: "infobox-properties", attrs: { "aria-label": "Property summary" } }, [
      element("h3", { text: "Property summary" }),
      element("dl", { className: "infobox-facts" }, summaryFacts.map((group) => infoboxProperty(group, references, reportTargets))),
    ]) : null,
    lazyDetails("Record and publication details", () => element("dl", { className: "infobox-facts" }, [
      ...[["Public ID", mineral.public_id], ["Review status", mineral.verification_status ? humanLabel(mineral.verification_status) : null], ["Source kind", mineral.source_kind ? humanLabel(mineral.source_kind) : null], ["Source status", mineral.source_status ? humanLabel(mineral.source_status) : null], ["License", mineral.license_spdx], ["Data quality", quality === null ? null : `${quality}%`], ["Evidence entries", formatNumber(evidenceResult.items.length)], ["Active offers", formatNumber(offers.length)]].filter(([, value]) => value !== undefined && value !== null && value !== "").map(([label, value]) => fact(label, value)),
    ])),
  ]);
  const contents = introduction || sections.length ? element("nav", { className: "mineral-contents", attrs: { "aria-label": "Mineral contents" } }, [
    element("h2", { text: "Contents" }),
    introduction ? articleJump("mineral-overview", "Overview") : null,
    ...sections.map(({ id, title }) => articleJump(id, title)),
  ]) : null;
  const view = element("article", { className: "record-view mineral-document" }, [
    element("div", { className: "mineral-toolbar" }, [routeLink("/minerals", `← ${t("catalog")}`, "mineral-back"), searchForm(normalizeSearchParams({}))]),
    element("header", { className: "mineral-heading", attrs: { "aria-labelledby": "record-title" } }, [
      paragraph("MINERAL ATLAS", "article-eyebrow"),
      element("h1", { id: "record-title", text: mineral.canonical_name }),
      element("div", { className: "article-subtitle" }, [
        mineral.mineral_family ? element("span", { text: mineral.mineral_family }) : null,
        mineral.nomenclature_status ? element("span", { text: humanLabel(mineral.nomenclature_status) }) : null,
        element("span", { text: `${formatNumber(references.entries.length)} reference${references.entries.length === 1 ? "" : "s"}` }),
      ]),
    ]),
    contents,
    element("div", { className: "mineral-article-body" }, [introduction, ...sections.map(({ node }) => node)]),
    infobox,
  ]);
  return { node: view, title: `${mineral.canonical_name} · Waajacu’s Minerals` };
}

function renderMapShell() {
  const container = element("div", { id: "catalog-map-root", className: "map-container", attrs: { "aria-busy": "true" } }, [paragraph("Loading map…", "visually-hidden")]);
  const node = element("section", { className: "map-view", attrs: { "aria-labelledby": "map-title" } }, [
    element("header", { className: "map-hero" }, [
      element("div", { className: "map-hero-copy" }, [
        paragraph("MAP / CURRENT DATA", "kicker kicker-gold"),
        element("h1", { id: "map-title", text: "Environmental context map." }),
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
        paragraph("The mineral atlas remains fully usable without the map package."),
        routeLink("/minerals", "OPEN THE ATLAS →", "button button-ink"),
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
        paragraph("SOURCE / RELEASE INFORMATION", "kicker kicker-gold"),
        element("h1", { id: "about-title" }, ["How the public", element("em", { text: "release works." })]),
        paragraph("Waajacu’s public atlas is a read-only projection of published mineral facts, evidence attribution, and public market offers.", "route-intro"),
      ]),
      image("./assets/atlas-source-v2.png", "", "about-source-art", "1920", "819", "eager"),
    ]),
    element("div", { className: "about-grid" }, [
      element("section", { className: "about-panel about-panel-dark" }, [
        paragraph("01 / LOCAL VERIFICATION", "kicker kicker-gold"),
        element("h2", { text: "Local release verification" }),
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
      element("h2", { text: "Evidence and provenance." }),
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
  if (node.matches?.(".mineral-document")) observeArticlePanels(node, routeController?.signal);
  document.title = title;
  updateManifestBindings();
  if (focus && hasRendered) main.focus({ preventScroll: true });
  hasRendered = true;
}

function observeArticlePanels(article, signal) {
  const panels = [
    { node: article.querySelector(".mineral-infobox"), query: "(min-width: 761px)" },
    { node: article.querySelector(".mineral-contents"), query: "(min-width: 1151px)" },
  ].filter(({ node }) => node);
  const update = () => {
    if (signal?.aborted || !article.isConnected) return;
    const inset = parseFloat(getComputedStyle(document.documentElement).fontSize) * 1.2;
    const availableHeight = document.documentElement.clientHeight - (2 * inset);
    for (const { node, query } of panels) {
      node.toggleAttribute("data-sticky", matchMedia(query).matches && node.getBoundingClientRect().height <= availableHeight);
    }
  };
  const observer = typeof ResizeObserver === "function" ? new ResizeObserver(update) : null;
  for (const { node } of panels) observer?.observe(node);
  addEventListener("resize", update, { passive: true, signal });
  signal?.addEventListener("abort", () => observer?.disconnect(), { once: true });
  requestAnimationFrame(update);
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
      finishRoute(renderHome(), "Waajacu’s Minerals — Public Mineral Atlas", { focus });
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
      finishRoute(node, "Atlas · Waajacu’s Minerals", { focus });
      announce("Atlas ready");
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
    finishRoute(errorView(error, () => renderCurrentRoute({ focus: false })), "Atlas unavailable · Waajacu’s Minerals", { focus });
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
