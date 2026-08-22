import {
  isOfferActiveAt,
  normalizeSearchParams,
  parseRoute,
  routeHref,
  validateWorkerRequest,
  validateWorkerResponse,
} from "./app-core.mjs";

const main = document.querySelector("#app-main");
const statusRegion = document.querySelector("#app-status");
const releaseSummary = document.querySelector("#release-summary");
const localeSelect = document.querySelector("#locale-select");
const themeToggle = document.querySelector("#theme-toggle");
const mapModuleMeta = document.querySelector('meta[name="waajacu-map-module"]');
const APP_BASE_PATH = new URL(".", import.meta.url).pathname;

const STORAGE_KEYS = Object.freeze({ theme: "waajacu.theme", locale: "waajacu.locale" });
const SUPPORTED_LOCALES = new Set(["en", "es", "de", "fr", "cs", "zh", "ar", "pt", "hi", "ja"]);
const THEMES = ["system", "light", "dark"];

const ENGLISH = Object.freeze({
  home: "Home", minerals: "Minerals", map: "Map", about: "About", theme: "Theme",
  search: "Search", searchLabel: "Search minerals", searchHint: "Name, formula, family, or keyword",
  viewRecord: "View record", previous: "Previous", next: "Next", evidence: "Evidence", offers: "Offers",
  details: "Details", noResults: "No minerals matched this search.", mapUnavailable: "The optional map is not available in this deployment.",
  opening: "Opening the verified catalog…", failed: "The public catalog could not be opened.", retry: "Try again",
});

const TRANSLATIONS = Object.freeze({
  en: ENGLISH,
  es: { home: "Inicio", minerals: "Minerales", map: "Mapa", about: "Acerca de", theme: "Tema", search: "Buscar", searchLabel: "Buscar minerales", searchHint: "Nombre, fórmula, familia o palabra clave", viewRecord: "Ver ficha", previous: "Anterior", next: "Siguiente", evidence: "Evidencia", offers: "Ofertas", details: "Detalles", noResults: "Ningún mineral coincide con la búsqueda.", mapUnavailable: "El mapa opcional no está disponible en esta publicación.", opening: "Abriendo el catálogo verificado…", failed: "No se pudo abrir el catálogo público.", retry: "Reintentar" },
  de: { home: "Start", minerals: "Minerale", map: "Karte", about: "Über uns", theme: "Design", search: "Suchen", searchLabel: "Minerale suchen", searchHint: "Name, Formel, Familie oder Stichwort", viewRecord: "Datensatz öffnen", previous: "Zurück", next: "Weiter", evidence: "Nachweise", offers: "Angebote", details: "Details", noResults: "Keine passenden Minerale gefunden.", mapUnavailable: "Die optionale Karte ist in dieser Bereitstellung nicht verfügbar.", opening: "Verifizierter Katalog wird geöffnet…", failed: "Der öffentliche Katalog konnte nicht geöffnet werden.", retry: "Erneut versuchen" },
  fr: { home: "Accueil", minerals: "Minéraux", map: "Carte", about: "À propos", theme: "Thème", search: "Rechercher", searchLabel: "Rechercher des minéraux", searchHint: "Nom, formule, famille ou mot-clé", viewRecord: "Voir la fiche", previous: "Précédent", next: "Suivant", evidence: "Sources", offers: "Offres", details: "Détails", noResults: "Aucun minéral ne correspond à cette recherche.", mapUnavailable: "La carte facultative n’est pas disponible dans ce déploiement.", opening: "Ouverture du catalogue vérifié…", failed: "Impossible d’ouvrir le catalogue public.", retry: "Réessayer" },
  cs: { home: "Domů", minerals: "Minerály", map: "Mapa", about: "O projektu", theme: "Motiv", search: "Hledat", searchLabel: "Hledat minerály", searchHint: "Název, vzorec, skupina nebo klíčové slovo", viewRecord: "Zobrazit záznam", previous: "Předchozí", next: "Další", evidence: "Zdroje", offers: "Nabídky", details: "Podrobnosti", noResults: "Vyhledávání neodpovídá žádný minerál.", mapUnavailable: "Volitelná mapa není v tomto nasazení dostupná.", opening: "Otevírání ověřeného katalogu…", failed: "Veřejný katalog se nepodařilo otevřít.", retry: "Zkusit znovu" },
  zh: { home: "首页", minerals: "矿物", map: "地图", about: "关于", theme: "主题", search: "搜索", searchLabel: "搜索矿物", searchHint: "名称、化学式、类别或关键词", viewRecord: "查看记录", previous: "上一页", next: "下一页", evidence: "证据", offers: "报价", details: "详情", noResults: "没有符合搜索条件的矿物。", mapUnavailable: "此部署未提供可选地图。", opening: "正在打开已验证目录…", failed: "无法打开公共目录。", retry: "重试" },
  ar: { home: "الرئيسية", minerals: "المعادن", map: "الخريطة", about: "حول", theme: "المظهر", search: "بحث", searchLabel: "البحث عن المعادن", searchHint: "الاسم أو الصيغة أو العائلة أو كلمة مفتاحية", viewRecord: "عرض السجل", previous: "السابق", next: "التالي", evidence: "الأدلة", offers: "العروض", details: "التفاصيل", noResults: "لا توجد معادن مطابقة لهذا البحث.", mapUnavailable: "الخريطة الاختيارية غير متاحة في هذا النشر.", opening: "جارٍ فتح الكتالوج المتحقق منه…", failed: "تعذر فتح الكتالوج العام.", retry: "إعادة المحاولة" },
  pt: { home: "Início", minerals: "Minerais", map: "Mapa", about: "Sobre", theme: "Tema", search: "Pesquisar", searchLabel: "Pesquisar minerais", searchHint: "Nome, fórmula, família ou palavra-chave", viewRecord: "Ver registro", previous: "Anterior", next: "Seguinte", evidence: "Evidências", offers: "Ofertas", details: "Detalhes", noResults: "Nenhum mineral corresponde à pesquisa.", mapUnavailable: "O mapa opcional não está disponível nesta implantação.", opening: "Abrindo o catálogo verificado…", failed: "Não foi possível abrir o catálogo público.", retry: "Tentar novamente" },
  hi: { home: "होम", minerals: "खनिज", map: "मानचित्र", about: "परिचय", theme: "थीम", search: "खोजें", searchLabel: "खनिज खोजें", searchHint: "नाम, सूत्र, परिवार या मुख्य शब्द", viewRecord: "रिकॉर्ड देखें", previous: "पिछला", next: "अगला", evidence: "साक्ष्य", offers: "प्रस्ताव", details: "विवरण", noResults: "इस खोज से कोई खनिज नहीं मिला।", mapUnavailable: "इस परिनियोजन में वैकल्पिक मानचित्र उपलब्ध नहीं है।", opening: "सत्यापित सूची खोली जा रही है…", failed: "सार्वजनिक सूची नहीं खोली जा सकी।", retry: "फिर प्रयास करें" },
  ja: { home: "ホーム", minerals: "鉱物", map: "地図", about: "概要", theme: "テーマ", search: "検索", searchLabel: "鉱物を検索", searchHint: "名前、化学式、分類、キーワード", viewRecord: "記録を見る", previous: "前へ", next: "次へ", evidence: "根拠", offers: "オファー", details: "詳細", noResults: "検索に一致する鉱物はありません。", mapUnavailable: "この配信にはオプションの地図がありません。", opening: "検証済みカタログを開いています…", failed: "公開カタログを開けませんでした。", retry: "再試行" },
});

function storedValue(key) {
  try { return localStorage.getItem(key); } catch { return null; }
}

function storeValue(key, value) {
  try { localStorage.setItem(key, value); } catch { /* Preferences remain session-local. */ }
}

function preferredLocale() {
  const stored = storedValue(STORAGE_KEYS.locale);
  if (stored && SUPPORTED_LOCALES.has(stored)) return stored;
  for (const candidate of navigator.languages ?? [navigator.language]) {
    const primary = String(candidate).toLowerCase().split("-")[0];
    if (SUPPORTED_LOCALES.has(primary)) return primary;
  }
  return "en";
}

const preferences = {
  locale: preferredLocale(),
  theme: THEMES.includes(storedValue(STORAGE_KEYS.theme)) ? storedValue(STORAGE_KEYS.theme) : "system",
};

function t(key) {
  return TRANSLATIONS[preferences.locale]?.[key] ?? ENGLISH[key] ?? key;
}

function element(tagName, options = {}, children = []) {
  const node = document.createElement(tagName);
  if (options.className) node.className = options.className;
  if (options.text !== undefined && options.text !== null) node.textContent = String(options.text);
  if (options.id) node.id = options.id;
  if (options.attrs) {
    for (const [name, value] of Object.entries(options.attrs)) {
      if (value !== undefined && value !== null && value !== false) node.setAttribute(name, value === true ? "" : String(value));
    }
  }
  const childList = Array.isArray(children) ? children : [children];
  for (const child of childList) {
    if (child instanceof Node) node.append(child);
    else if (child !== undefined && child !== null) node.append(document.createTextNode(String(child)));
  }
  return node;
}

function paragraph(text, className) {
  return element("p", { text, className });
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

function externalLink(value, label) {
  const href = safeHttpUrl(value);
  return href
    ? element("a", { text: label, attrs: { href, target: "_blank", rel: "noopener noreferrer" } })
    : paragraph(label, "muted");
}

function routeLink(path, label, className) {
  return element("a", {
    text: label,
    className,
    attrs: { href: routeHref(path), "data-route-link": "" },
  });
}

function announce(message) {
  statusRegion.textContent = "";
  requestAnimationFrame(() => { statusRegion.textContent = message; });
}

function resolvedTheme() {
  if (preferences.theme !== "system") return preferences.theme;
  return matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function applyPreferences() {
  document.documentElement.dataset.theme = preferences.theme;
  document.documentElement.lang = preferences.locale;
  document.documentElement.dir = preferences.locale === "ar" ? "rtl" : "ltr";
  localeSelect.value = preferences.locale;
  themeToggle.querySelector("[data-theme-label]").textContent = `${t("theme")}: ${preferences.theme}`;
  themeToggle.setAttribute("aria-label", `${t("theme")}: ${preferences.theme}`);
  for (const [name, key] of [["home", "home"], ["minerals", "minerals"], ["map", "map"], ["about", "about"]]) {
    const link = document.querySelector(`[data-nav="${name}"]`);
    link.textContent = t(key);
    link.href = routeHref(name === "home" ? "/" : `/${name}`);
  }
  const brand = document.querySelector(".brand");
  brand.href = routeHref("/");
  document.querySelector(".skip-link").textContent = "Skip to main content";
  if (manifest) updateReleaseSummary();
}

class CatalogClient {
  #worker;
  #nextId = 1;
  #pending = new Map();

  constructor() {
    this.#worker = new Worker(new URL("./catalog-worker.js", import.meta.url), { type: "module", name: "waajacu-catalog" });
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

  init() { return this.request("init", { manifestUrl: new URL("./catalog-manifest.json", import.meta.url).href }); }
  search(input, signal) {
    const normalized = normalizeSearchParams(input);
    return this.request("search", normalized, signal);
  }
  detail(slug, signal) { return this.request("detail", { slug }, signal); }
  evidence(slug, signal) { return this.request("evidence", { slug }, signal); }
  offers(slug, signal) { return this.request("offers", { slug }, signal); }
}

const catalog = new CatalogClient();
let catalogInit;
let manifest;
let renderSequence = 0;
let routeController;
let mapLifecycle;
let hasRendered = false;

async function ensureCatalog() {
  if (!catalogInit) {
    catalogInit = catalog.init().then((result) => {
      manifest = result.manifest;
      updateReleaseSummary();
      return result;
    }).catch((error) => {
      catalogInit = undefined;
      throw error;
    });
  }
  return catalogInit;
}

function updateReleaseSummary() {
  const generated = formatDate(manifest.generated_at);
  releaseSummary.textContent = `${manifest.mineral_count.toLocaleString(preferences.locale)} minerals · ${generated}`;
}

function formatDate(value) {
  const date = new Date(value);
  return Number.isFinite(date.valueOf())
    ? new Intl.DateTimeFormat(preferences.locale, { dateStyle: "medium" }).format(date)
    : String(value ?? "");
}

function titleFor(route, mineralName) {
  const label = mineralName ?? ({ home: t("home"), minerals: t("minerals"), map: t("map"), about: t("about") }[route.name] ?? "Not found");
  document.title = route.name === "home" ? "Waajacu's Minerals" : `${label} · Waajacu's Minerals`;
}

function setActiveNavigation(route) {
  const active = route.name === "mineral" ? "minerals" : route.name;
  for (const link of document.querySelectorAll("[data-nav]")) {
    if (link.dataset.nav === active) link.setAttribute("aria-current", "page");
    else link.removeAttribute("aria-current");
  }
}

function loadingView() {
  return element("section", { className: "view loading-panel", attrs: { "aria-busy": "true" } }, [
    paragraph("Public catalog", "eyebrow"),
    element("h1", { text: t("opening") }),
    paragraph("The database is being checked locally before any record is shown."),
  ]);
}

function errorView(error, retry) {
  const section = element("section", { className: "view error-panel" }, [
    paragraph("Catalog unavailable", "eyebrow"),
    element("h1", { text: t("failed") }),
    paragraph(error instanceof Error ? error.message : "An unknown error occurred."),
  ]);
  const button = element("button", { className: "primary-button", text: t("retry"), attrs: { type: "button" } });
  button.addEventListener("click", retry);
  section.append(button);
  return section;
}

function badge(value, className = "") {
  return element("span", { className: `badge ${className}`.trim(), text: value });
}

function statusBadges(mineral) {
  const list = element("div", { className: "badge-list", attrs: { "aria-label": "Record status" } });
  for (const value of [mineral.mineral_family, mineral.nomenclature_status, mineral.verification_status]) {
    if (value) list.append(badge(value));
  }
  return list;
}

function mineralCard(mineral) {
  const headingId = `mineral-${mineral.slug}`;
  const article = element("article", { className: "mineral-card", attrs: { "aria-labelledby": headingId } });
  const heading = element("h2", { id: headingId });
  heading.append(routeLink(`/minerals/${encodeURIComponent(mineral.slug)}`, mineral.canonical_name, "card-link"));
  article.append(
    statusBadges(mineral),
    heading,
    paragraph(mineral.formula || "Formula not published", "mineral-formula"),
    paragraph(mineral.description_excerpt || "No public description is available.", "card-description"),
    element("div", { className: "card-meta" }, [
      paragraph(`${Number(mineral.evidence_count) || 0} ${t("evidence").toLowerCase()}`),
      paragraph(`${Number(mineral.active_offer_count) || 0} ${t("offers").toLowerCase()}`),
    ]),
    routeLink(`/minerals/${encodeURIComponent(mineral.slug)}`, t("viewRecord"), "secondary-button"),
  );
  return article;
}

function mineralGrid(items) {
  return element("div", { className: "mineral-grid" }, items.map(mineralCard));
}

async function renderHome(signal) {
  const result = await catalog.search({ query: "", page: 1, pageSize: 6 }, signal);
  const section = element("section", { className: "view" });
  const hero = element("div", { className: "hero" }, [
    element("div", { className: "hero-copy" }, [
      paragraph("Public catalog · independently verifiable", "eyebrow"),
      element("h1", { text: "Mineral knowledge, with its evidence intact." }),
      paragraph("Explore a compact public snapshot of mineral records, their source context, and current published offers.", "hero-lede"),
      element("div", { className: "actions" }, [
        routeLink("/minerals", t("minerals"), "primary-button"),
        routeLink("/about", t("about"), "secondary-button"),
      ]),
    ]),
    element("aside", { className: "release-card", attrs: { "aria-label": "Catalog release" } }, [
      paragraph("Verified snapshot", "eyebrow"),
      element("strong", { text: manifest.mineral_count.toLocaleString(preferences.locale) }),
      paragraph("public mineral records"),
      paragraph(formatDate(manifest.generated_at), "muted"),
    ]),
  ]);
  section.append(hero, element("div", { className: "section-heading" }, [
    element("div", {}, [paragraph("Collection", "eyebrow"), element("h2", { text: "Browse the catalog" })]),
    routeLink("/minerals", `${t("minerals")} →`, "text-link"),
  ]));
  if (result.items.length) section.append(mineralGrid(result.items));
  return section;
}

function searchForm(search) {
  const form = element("form", { className: "search-form", attrs: { role: "search" } });
  const label = element("label", { text: t("searchLabel"), attrs: { for: "catalog-search" } });
  const row = element("div", { className: "search-row" });
  const input = element("input", {
    id: "catalog-search",
    attrs: { type: "search", name: "q", value: search.query, placeholder: t("searchHint"), maxlength: "160", autocomplete: "off" },
  });
  const submit = element("button", { text: t("search"), className: "primary-button", attrs: { type: "submit" } });
  row.append(input, submit);
  form.append(label, row);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    navigate(routeHref("/minerals", { q: input.value, page: "1", page_size: String(search.pageSize) }));
  });
  return form;
}

function pager(search, result) {
  if (result.total_pages <= 1) return null;
  const nav = element("nav", { className: "pager", attrs: { "aria-label": "Search result pages" } });
  const paramsFor = (page) => ({ q: search.query, page: String(page), page_size: String(search.pageSize) });
  if (search.page > 1) nav.append(routeLink(`/minerals?${new URLSearchParams(paramsFor(search.page - 1))}`, `← ${t("previous")}`, "secondary-button"));
  nav.append(paragraph(`${Math.min(search.page, result.total_pages)} / ${result.total_pages}`, "pager-status"));
  if (search.page < result.total_pages) nav.append(routeLink(`/minerals?${new URLSearchParams(paramsFor(search.page + 1))}`, `${t("next")} →`, "secondary-button"));
  return nav;
}

async function renderMinerals(route, signal) {
  const result = await catalog.search(route.search, signal);
  const section = element("section", { className: "view" }, [
    paragraph("Public catalog", "eyebrow"),
    element("h1", { text: t("minerals") }),
    paragraph("Search the immutable release by name, chemical formula, mineral family, or indexed keyword.", "view-intro"),
    searchForm(route.search),
  ]);
  const summary = result.total === 1 ? "1 mineral" : `${result.total.toLocaleString(preferences.locale)} minerals`;
  section.append(paragraph(route.search.query ? `${summary} for “${route.search.query}”` : summary, "search-summary"));
  if (result.items.length) section.append(mineralGrid(result.items));
  else section.append(element("div", { className: "empty-panel" }, [element("h2", { text: t("noResults") }), paragraph("Try a shorter name, a formula, or another keyword.")]));
  const pagination = pager(route.search, result);
  if (pagination) section.append(pagination);
  announce(summary);
  return section;
}

function humanLabel(value) {
  return String(value).replaceAll("_", " ").replace(/^./u, (character) => character.toUpperCase());
}

function fact(label, value) {
  const card = element("div", { className: "fact-card" }, [element("dt", { text: label }), element("dd", { text: value ?? "—" })]);
  return card;
}

function parsedJson(text) {
  if (typeof text !== "string" || text.length === 0) return null;
  try { return JSON.parse(text); } catch { return null; }
}

function structuredData(value, depth = 0) {
  if (depth > 4) return paragraph("…", "muted");
  if (value === null || typeof value !== "object") return element("span", { text: value === null ? "—" : String(value) });
  if (Array.isArray(value)) {
    const list = element("ul", { className: "data-list" });
    for (const item of value.slice(0, 100)) list.append(element("li", {}, structuredData(item, depth + 1)));
    return list;
  }
  const list = element("dl", { className: "data-list" });
  for (const [key, item] of Object.entries(value).slice(0, 100)) {
    list.append(element("div", {}, [element("dt", { text: humanLabel(key) }), element("dd", {}, structuredData(item, depth + 1))]));
  }
  return list;
}

function jsonSection(title, raw) {
  const data = parsedJson(raw);
  if (data === null || (Array.isArray(data) && data.length === 0) || (typeof data === "object" && !Array.isArray(data) && Object.keys(data).length === 0)) return null;
  return element("section", { className: "record-section" }, [element("h2", { text: title }), structuredData(data)]);
}

function evidenceCard(item, index) {
  const title = item.title || item.work_title || `Evidence ${index + 1}`;
  const article = element("article", { className: "evidence-card" }, [
    element("div", { className: "badge-list" }, [item.review_status ? badge(item.review_status) : null, item.license_spdx ? badge(item.license_spdx) : null].filter(Boolean)),
    element("h3", { text: title }),
    paragraph([item.publisher, item.attribution_party].filter(Boolean).join(" · "), "muted"),
  ]);
  const url = item.canonical_url || item.work_url;
  if (url) article.append(externalLink(url, "Open source ↗"));
  const claim = parsedJson(item.claim_json);
  if (claim !== null) article.append(element("div", { className: "evidence-claim" }, [element("h4", { text: item.claim_scope || "Claim" }), structuredData(claim)]));
  const notices = [item.changes_notice, item.no_endorsement_notice].filter(Boolean);
  for (const notice of notices) article.append(paragraph(notice, "notice"));
  return article;
}

function priceText(item) {
  const exponent = Number(item.currency_exponent);
  const minor = Number(item.price_minor);
  if (Number.isSafeInteger(minor) && Number.isInteger(exponent) && exponent >= 0 && exponent <= 6 && /^[A-Z]{3}$/.test(item.currency_code ?? "")) {
    try {
      return new Intl.NumberFormat(preferences.locale, { style: "currency", currency: item.currency_code }).format(minor / (10 ** exponent));
    } catch { /* Use the exact minor-unit fallback below. */ }
  }
  return item.price_minor === null ? "Price on request" : `${item.price_minor} ${item.currency_code ?? ""}`.trim();
}

function offerCard(item) {
  const article = element("article", { className: "offer-card" }, [
    element("div", { className: "badge-list" }, [item.stock_status ? badge(item.stock_status) : null, item.verification_status ? badge(item.verification_status) : null].filter(Boolean)),
    element("h3", { text: item.title || item.provider_name || "Published offer" }),
    paragraph(item.provider_name || "Provider", "muted"),
    paragraph(priceText(item), "price"),
  ]);
  const facts = [
    ["Basis", item.pricing_basis], ["Minimum order", [item.minimum_order_quantity, item.minimum_order_unit].filter(Boolean).join(" ")],
    ["Purity", item.purity_text], ["Grade", item.grade], ["Origin", item.origin_country_code], ["Checked", item.last_checked_at ? formatDate(item.last_checked_at) : null],
  ].filter(([, value]) => value !== null && value !== undefined && value !== "");
  if (facts.length) article.append(element("dl", { className: "compact-facts" }, facts.map(([label, value]) => element("div", {}, [element("dt", { text: label }), element("dd", { text: value })]))));
  if (item.product_url) article.append(externalLink(item.product_url, "Open provider page ↗"));
  return article;
}

async function renderMineral(route, signal) {
  const [mineral, evidenceResult, offerResult] = await Promise.all([
    catalog.detail(route.slug, signal), catalog.evidence(route.slug, signal), catalog.offers(route.slug, signal),
  ]);
  if (!mineral) return renderNotFound("That mineral is not part of this public release.");
  titleFor(route, mineral.canonical_name);
  const section = element("article", { className: "view mineral-detail" });
  const header = element("header", { className: "detail-header" }, [
    routeLink("/minerals", `← ${t("minerals")}`, "text-link"),
    statusBadges(mineral),
    element("h1", { text: mineral.canonical_name }),
    paragraph(mineral.formula || "Formula not published", "mineral-formula detail-formula"),
    paragraph(mineral.description || "No public description is available.", "detail-description"),
  ]);
  const facts = element("dl", { className: "fact-grid" }, [
    fact("Public ID", mineral.public_id), fact("CAS number", mineral.cas_number), fact("Mineral family", mineral.mineral_family),
    fact("Discovery country", mineral.discovery_country), fact("Source kind", mineral.source_kind), fact("License", mineral.license_spdx),
    fact("Data quality", mineral.data_quality_score), fact("Source status", mineral.source_status),
  ]);
  section.append(header, element("section", { className: "record-section" }, [element("h2", { text: t("details") }), facts]));
  for (const block of [jsonSection("Identifiers", mineral.identifiers_json), jsonSection("Properties", mineral.properties_json), jsonSection("Safety", mineral.safety_json)]) {
    if (block) section.append(block);
  }
  if (mineral.first_reference || mineral.second_reference) {
    section.append(element("section", { className: "record-section" }, [element("h2", { text: "References" }), element("ul", { className: "reference-list" }, [mineral.first_reference, mineral.second_reference].filter(Boolean).map((value) => element("li", { text: value })))]));
  }
  section.append(element("section", { className: "record-section" }, [
    element("div", { className: "section-heading" }, [element("h2", { text: t("evidence") }), badge(String(evidenceResult.items.length))]),
    evidenceResult.items.length ? element("div", { className: "evidence-list" }, evidenceResult.items.map(evidenceCard)) : paragraph("No public evidence records are attached to this release.", "empty-panel"),
  ]));
  const offers = offerResult.items.filter((item) => isOfferActiveAt(item.expires_at));
  section.append(element("section", { className: "record-section" }, [
    element("div", { className: "section-heading" }, [element("h2", { text: t("offers") }), badge(String(offers.length))]),
    offers.length ? element("div", { className: "offer-grid" }, offers.map(offerCard)) : paragraph("No unexpired public offers are available.", "empty-panel"),
  ]));
  return section;
}

function renderAbout() {
  return element("section", { className: "view prose-view" }, [
    paragraph("About this catalog", "eyebrow"),
    element("h1", { text: "Evidence-forward mineral knowledge" }),
    paragraph("Waajacu’s public catalog is a deliberately limited, read-only projection. It contains public mineral facts, evidence attribution, and published market offers; operational accounts, review queues, and private registry data stay outside this browser application."),
    element("h2", { text: "Verified in your browser" }),
    paragraph("Every release names a content-addressed SQLite database. A dedicated worker checks its byte length and SHA-256 digest, opens it read-only with official SQLite WebAssembly, and validates the v1 schema before answering fixed, parameterized queries."),
    element("h2", { text: "Release" }),
    element("dl", { className: "fact-grid" }, [
      fact("Format", manifest.format), fact("Schema", manifest.schema_version), fact("Generated", formatDate(manifest.generated_at)),
      fact("Minerals", manifest.mineral_count.toLocaleString(preferences.locale)), fact("Release ID", manifest.release_id), fact("Database SHA-256", manifest.database.sha256),
    ]),
    paragraph("Source licenses and attribution notices remain attached to their records. Follow each evidence link for the authoritative source and its complete terms.", "notice"),
  ]);
}

function renderNotFound(message = "This route does not exist.") {
  return element("section", { className: "view empty-panel" }, [
    paragraph("404", "eyebrow"), element("h1", { text: "Page not found" }), paragraph(message), routeLink("/", t("home"), "primary-button"),
  ]);
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

async function renderMap(sequence) {
  const section = element("section", {
    className: "view map-shell",
    attrs: { "aria-label": t("map") },
  });
  const container = element("div", {
    id: "catalog-map-root",
    className: "map-container",
    attrs: { "aria-busy": "true" },
  }, [paragraph("Loading map…", "visually-hidden")]);
  section.append(container);
  main.replaceChildren(section);

  const controller = new AbortController();
  mapLifecycle = { controller, cleanup: undefined };
  try {
    const configured = mapModuleMeta?.content?.trim();
    if (!configured) throw new Error("No map module is configured.");
    const moduleUrl = new URL(configured, import.meta.url);
    if (moduleUrl.origin !== location.origin || moduleUrl.username || moduleUrl.password || moduleUrl.hash) throw new Error("The map module URL must be same-origin.");
    const module = await import(moduleUrl.href);
    if (controller.signal.aborted || sequence !== renderSequence) return section;
    if (typeof module.mountMineralsMap !== "function") throw new Error("The map module does not export mountMineralsMap().");
    container.replaceChildren();
    container.removeAttribute("aria-busy");
    const cleanup = await module.mountMineralsMap(container, {
      theme: resolvedTheme(),
      signal: controller.signal,
    });
    if (cleanup !== undefined && typeof cleanup !== "function") throw new Error("The map mount function returned an invalid cleanup value.");
    if (controller.signal.aborted || sequence !== renderSequence) {
      if (typeof cleanup === "function") await cleanup();
    } else {
      mapLifecycle.cleanup = cleanup;
    }
  } catch (error) {
    if (!controller.signal.aborted && sequence === renderSequence) {
      container.removeAttribute("aria-busy");
      container.replaceChildren(element("div", { className: "empty-panel" }, [element("h2", { text: t("mapUnavailable") }), paragraph("Browse the mineral list while the optional map package is absent."), routeLink("/minerals", t("minerals"), "secondary-button")]));
      console.info("Optional map module unavailable:", error);
    }
  }
  return section;
}

async function renderCurrentRoute({ focus = true } = {}) {
  const sequence = ++renderSequence;
  routeController?.abort();
  routeController = new AbortController();
  await teardownMap();
  const route = parseRoute(location.href, APP_BASE_PATH);
  applyPreferences();
  setActiveNavigation(route);
  titleFor(route);
  main.replaceChildren(loadingView());
  try {
    await ensureCatalog();
    if (sequence !== renderSequence) return;
    let content;
    switch (route.name) {
      case "home": content = await renderHome(routeController.signal); break;
      case "minerals": content = await renderMinerals(route, routeController.signal); break;
      case "mineral": content = await renderMineral(route, routeController.signal); break;
      case "about": content = renderAbout(); break;
      case "map":
        await renderMap(sequence);
        content = null;
        break;
      default: content = renderNotFound();
    }
    if (sequence !== renderSequence || routeController.signal.aborted) return;
    if (content) main.replaceChildren(content);
    if (focus && hasRendered) main.focus({ preventScroll: true });
    hasRendered = true;
  } catch (error) {
    if (error?.name === "AbortError" || sequence !== renderSequence) return;
    main.replaceChildren(errorView(error, () => renderCurrentRoute({ focus: false })));
    titleFor({ name: "not-found" });
    announce(t("failed"));
  }
}

function navigate(destination, options = {}) {
  let href = destination;
  if (destination instanceof URL) href = destination.href;
  if (typeof href !== "string") throw new TypeError("Navigation destination must be a URL or string.");
  if (href.startsWith("/")) href = routeHref(href);
  const target = new URL(href, location.href);
  if (target.origin !== location.origin) {
    location.assign(target.href);
    return;
  }
  const state = { waajacuRoute: true };
  if (options.replace) history.replaceState(state, "", target.href);
  else history.pushState(state, "", target.href);
  renderCurrentRoute();
}

document.addEventListener("click", (event) => {
  if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
  const link = event.target.closest("a[data-route-link]");
  if (!link || link.target === "_blank" || link.hasAttribute("download")) return;
  const url = new URL(link.href, location.href);
  if (url.origin !== location.origin) return;
  event.preventDefault();
  navigate(url);
});

document.querySelector(".skip-link").addEventListener("click", (event) => {
  event.preventDefault();
  main.focus({ preventScroll: true });
  main.scrollIntoView({ block: "start" });
});

localeSelect.addEventListener("change", () => {
  if (!SUPPORTED_LOCALES.has(localeSelect.value)) return;
  preferences.locale = localeSelect.value;
  storeValue(STORAGE_KEYS.locale, preferences.locale);
  renderCurrentRoute({ focus: false });
});

themeToggle.addEventListener("click", () => {
  preferences.theme = THEMES[(THEMES.indexOf(preferences.theme) + 1) % THEMES.length];
  storeValue(STORAGE_KEYS.theme, preferences.theme);
  renderCurrentRoute({ focus: false });
});

matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
  if (preferences.theme === "system" && parseRoute(location.href, APP_BASE_PATH).name === "map") renderCurrentRoute({ focus: false });
});

addEventListener("popstate", () => renderCurrentRoute());
addEventListener("hashchange", () => renderCurrentRoute());
applyPreferences();
renderCurrentRoute({ focus: false });
