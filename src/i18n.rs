#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    En,
    Es,
    Cs,
    Zh,
    Ar,
    Fr,
    De,
    Pt,
    Hi,
    Ja,
}

impl Language {
    pub fn all() -> &'static [Language] {
        &[
            Language::En,
            Language::Es,
            Language::Cs,
            Language::De,
            Language::Fr,
            Language::Zh,
            Language::Ar,
            Language::Pt,
            Language::Hi,
            Language::Ja,
        ]
    }

    pub fn code(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::Cs => "cs",
            Language::Es => "es",
            Language::De => "de",
            Language::Fr => "fr",

            Language::Zh => "zh",
            Language::Ar => "ar",
            Language::Pt => "pt",
            Language::Hi => "hi",
            Language::Ja => "ja",
        }
    }

    pub fn dir(self) -> &'static str {
        match self {
            Language::Ar => "rtl",
            _ => "ltr",
        }
    }

    pub fn english_name(self) -> &'static str {
        match self {
            Language::En => "English",
            Language::Es => "Spanish",
            Language::Cs => "Czech",
            Language::Zh => "Chinese",
            Language::Ar => "Arabic",
            Language::Fr => "French",
            Language::De => "German",
            Language::Pt => "Portuguese",
            Language::Hi => "Hindi",
            Language::Ja => "Japanese",
        }
    }

    pub fn native_name(self) -> &'static str {
        match self {
            Language::En => "English",
            Language::Es => "Español",
            Language::Cs => "Čeština",
            Language::Zh => "中文",
            Language::Ar => "العربية",
            Language::Fr => "Français",
            Language::De => "Deutsch",
            Language::Pt => "Português",
            Language::Hi => "हिन्दी",
            Language::Ja => "日本語",
        }
    }

    pub fn from_code(value: &str) -> Option<Self> {
        let code = value
            .trim()
            .to_ascii_lowercase()
            .split('-')
            .next()
            .unwrap_or_default()
            .to_string();

        match code.as_str() {
            "en" => Some(Language::En),
            "es" => Some(Language::Es),
            "cs" => Some(Language::Cs),
            "fr" => Some(Language::Fr),
            "de" => Some(Language::De),

            "zh" => Some(Language::Zh),
            "ar" => Some(Language::Ar),
            "pt" => Some(Language::Pt),
            "hi" => Some(Language::Hi),
            "ja" => Some(Language::Ja),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LanguageOption {
    pub code: &'static str,
    pub label: &'static str,
}

pub fn language_options() -> Vec<LanguageOption> {
    Language::all()
        .iter()
        .map(|lang| LanguageOption {
            code: lang.code(),
            label: lang.native_name(),
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
struct MaterialFactLabels {
    appearance: &'static str,
    boiling_point_c: &'static str,
    color: &'static str,
    crystal_system: &'static str,
    density_g_cm3: &'static str,
    disposal: &'static str,
    first_aid: &'static str,
    handling: &'static str,
    hardness_mohs: &'static str,
    hazards: &'static str,
    luster: &'static str,
    major_elements_pct: &'static str,
    melting_point_c: &'static str,
    molar_mass_g_mol: &'static str,
    notes: &'static str,
    ppe: &'static str,
    storage: &'static str,
    streak: &'static str,
}

pub fn material_fact_label(language: Language, key: &str) -> Option<&'static str> {
    let labels = material_fact_labels(language);
    match key.trim().to_ascii_lowercase().as_str() {
        "appearance" => Some(labels.appearance),
        "boiling_point_c" => Some(labels.boiling_point_c),
        "color" | "colour" => Some(labels.color),
        "crystal_system" => Some(labels.crystal_system),
        "density_g_cm3" => Some(labels.density_g_cm3),
        "disposal" => Some(labels.disposal),
        "first_aid" => Some(labels.first_aid),
        "handling" => Some(labels.handling),
        "hardness_mohs" => Some(labels.hardness_mohs),
        "hazards" => Some(labels.hazards),
        "luster" | "lustre" => Some(labels.luster),
        "major_elements_pct" => Some(labels.major_elements_pct),
        "melting_point_c" => Some(labels.melting_point_c),
        "molar_mass_g_mol" => Some(labels.molar_mass_g_mol),
        "notes" => Some(labels.notes),
        "ppe" => Some(labels.ppe),
        "storage" => Some(labels.storage),
        "streak" => Some(labels.streak),
        _ => None,
    }
}

fn material_fact_labels(language: Language) -> MaterialFactLabels {
    match language {
        Language::En => MaterialFactLabels {
            appearance: "Appearance",
            boiling_point_c: "Boiling point (°C)",
            color: "Color",
            crystal_system: "Crystal system",
            density_g_cm3: "Density (g/cm³)",
            disposal: "Disposal",
            first_aid: "First aid",
            handling: "Handling",
            hardness_mohs: "Hardness (Mohs)",
            hazards: "Hazards",
            luster: "Luster",
            major_elements_pct: "Major elements (%)",
            melting_point_c: "Melting point (°C)",
            molar_mass_g_mol: "Molar mass (g/mol)",
            notes: "Notes",
            ppe: "Protective equipment",
            storage: "Storage",
            streak: "Streak",
        },
        Language::Es => MaterialFactLabels {
            appearance: "Aspecto",
            boiling_point_c: "Punto de ebullición (°C)",
            color: "Color",
            crystal_system: "Sistema cristalino",
            density_g_cm3: "Densidad (g/cm³)",
            disposal: "Eliminación",
            first_aid: "Primeros auxilios",
            handling: "Manipulación",
            hardness_mohs: "Dureza (Mohs)",
            hazards: "Peligros",
            luster: "Brillo",
            major_elements_pct: "Elementos principales (%)",
            melting_point_c: "Punto de fusión (°C)",
            molar_mass_g_mol: "Masa molar (g/mol)",
            notes: "Notas",
            ppe: "Equipo de protección",
            storage: "Almacenamiento",
            streak: "Raya",
        },
        Language::Cs => MaterialFactLabels {
            appearance: "Vzhled",
            boiling_point_c: "Bod varu (°C)",
            color: "Barva",
            crystal_system: "Krystalová soustava",
            density_g_cm3: "Hustota (g/cm³)",
            disposal: "Likvidace",
            first_aid: "První pomoc",
            handling: "Manipulace",
            hardness_mohs: "Tvrdost (Mohs)",
            hazards: "Nebezpečí",
            luster: "Lesk",
            major_elements_pct: "Hlavní prvky (%)",
            melting_point_c: "Bod tání (°C)",
            molar_mass_g_mol: "Molární hmotnost (g/mol)",
            notes: "Poznámky",
            ppe: "Ochranné prostředky",
            storage: "Skladování",
            streak: "Vryp",
        },
        Language::De => MaterialFactLabels {
            appearance: "Erscheinungsbild",
            boiling_point_c: "Siedepunkt (°C)",
            color: "Farbe",
            crystal_system: "Kristallsystem",
            density_g_cm3: "Dichte (g/cm³)",
            disposal: "Entsorgung",
            first_aid: "Erste Hilfe",
            handling: "Handhabung",
            hardness_mohs: "Härte (Mohs)",
            hazards: "Gefahren",
            luster: "Glanz",
            major_elements_pct: "Hauptelemente (%)",
            melting_point_c: "Schmelzpunkt (°C)",
            molar_mass_g_mol: "Molare Masse (g/mol)",
            notes: "Hinweise",
            ppe: "Schutzausrüstung",
            storage: "Lagerung",
            streak: "Strichfarbe",
        },
        Language::Fr => MaterialFactLabels {
            appearance: "Aspect",
            boiling_point_c: "Point d’ébullition (°C)",
            color: "Couleur",
            crystal_system: "Système cristallin",
            density_g_cm3: "Masse volumique (g/cm³)",
            disposal: "Élimination",
            first_aid: "Premiers secours",
            handling: "Manipulation",
            hardness_mohs: "Dureté (Mohs)",
            hazards: "Dangers",
            luster: "Éclat",
            major_elements_pct: "Éléments majeurs (%)",
            melting_point_c: "Point de fusion (°C)",
            molar_mass_g_mol: "Masse molaire (g/mol)",
            notes: "Notes",
            ppe: "Équipement de protection",
            storage: "Stockage",
            streak: "Trait",
        },
        Language::Zh => MaterialFactLabels {
            appearance: "外观",
            boiling_point_c: "沸点 (°C)",
            color: "颜色",
            crystal_system: "晶系",
            density_g_cm3: "密度 (g/cm³)",
            disposal: "废弃处置",
            first_aid: "急救",
            handling: "操作注意事项",
            hardness_mohs: "莫氏硬度",
            hazards: "危害",
            luster: "光泽",
            major_elements_pct: "主要元素 (%)",
            melting_point_c: "熔点 (°C)",
            molar_mass_g_mol: "摩尔质量 (g/mol)",
            notes: "备注",
            ppe: "防护装备",
            storage: "储存",
            streak: "条痕",
        },
        Language::Ar => MaterialFactLabels {
            appearance: "المظهر",
            boiling_point_c: "درجة الغليان (°C)",
            color: "اللون",
            crystal_system: "النظام البلوري",
            density_g_cm3: "الكثافة (g/cm³)",
            disposal: "التخلص الآمن",
            first_aid: "الإسعافات الأولية",
            handling: "التعامل",
            hardness_mohs: "الصلادة (موهس)",
            hazards: "المخاطر",
            luster: "البريق",
            major_elements_pct: "العناصر الرئيسية (%)",
            melting_point_c: "درجة الانصهار (°C)",
            molar_mass_g_mol: "الكتلة المولية (g/mol)",
            notes: "ملاحظات",
            ppe: "معدات الوقاية",
            storage: "التخزين",
            streak: "المخدش",
        },
        Language::Pt => MaterialFactLabels {
            appearance: "Aparência",
            boiling_point_c: "Ponto de ebulição (°C)",
            color: "Cor",
            crystal_system: "Sistema cristalino",
            density_g_cm3: "Densidade (g/cm³)",
            disposal: "Descarte",
            first_aid: "Primeiros socorros",
            handling: "Manuseio",
            hardness_mohs: "Dureza (Mohs)",
            hazards: "Perigos",
            luster: "Brilho",
            major_elements_pct: "Elementos principais (%)",
            melting_point_c: "Ponto de fusão (°C)",
            molar_mass_g_mol: "Massa molar (g/mol)",
            notes: "Notas",
            ppe: "Equipamento de proteção",
            storage: "Armazenamento",
            streak: "Traço",
        },
        Language::Hi => MaterialFactLabels {
            appearance: "दिखावट",
            boiling_point_c: "क्वथनांक (°C)",
            color: "रंग",
            crystal_system: "क्रिस्टल तंत्र",
            density_g_cm3: "घनत्व (g/cm³)",
            disposal: "निपटान",
            first_aid: "प्राथमिक उपचार",
            handling: "सुरक्षित संचालन",
            hardness_mohs: "कठोरता (मोह्स)",
            hazards: "खतरे",
            luster: "चमक",
            major_elements_pct: "प्रमुख तत्व (%)",
            melting_point_c: "गलनांक (°C)",
            molar_mass_g_mol: "मोलर द्रव्यमान (g/mol)",
            notes: "टिप्पणियाँ",
            ppe: "सुरक्षात्मक उपकरण",
            storage: "भंडारण",
            streak: "लकीर का रंग",
        },
        Language::Ja => MaterialFactLabels {
            appearance: "外観",
            boiling_point_c: "沸点 (°C)",
            color: "色",
            crystal_system: "結晶系",
            density_g_cm3: "密度 (g/cm³)",
            disposal: "廃棄",
            first_aid: "応急処置",
            handling: "取扱い",
            hardness_mohs: "硬度 (モース)",
            hazards: "危険性",
            luster: "光沢",
            major_elements_pct: "主要元素 (%)",
            melting_point_c: "融点 (°C)",
            molar_mass_g_mol: "モル質量 (g/mol)",
            notes: "注記",
            ppe: "保護具",
            storage: "保管",
            streak: "条痕",
        },
    }
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // Some labels are retained solely for compatibility templates.
pub struct RegistryText {
    pub theme_toggle: &'static str,
    pub eyebrow: &'static str,
    pub title: &'static str,
    pub subtitle: &'static str,
    pub source_links: &'static str,
    pub buying_options: &'static str,
    pub search_label: &'static str,
    pub search_placeholder: &'static str,
    pub search_action: &'static str,
    pub empty_results: &'static str,
    pub pagination_showing: &'static str,
    pub pagination_of: &'static str,
    pub pagination_results: &'static str,
    pub pagination_page: &'static str,
    pub pagination_previous: &'static str,
    pub pagination_next: &'static str,
    pub kind_mineral: &'static str,
    pub ima_number: &'static str,
    pub ima_symbol: &'static str,
    pub mineral_species: &'static str,
    pub nomenclature_ima_approved: &'static str,
    pub nomenclature_recognized: &'static str,
    pub nomenclature_redefined: &'static str,
    pub nomenclature_renamed: &'static str,
    pub nomenclature_uncertain: &'static str,
    pub nomenclature_questionable: &'static str,
    pub nomenclature_discredited: &'static str,
    pub nomenclature_unknown: &'static str,
    pub valid_mineral_species: &'static str,
    pub mineral_facts: &'static str,
    pub nomenclature: &'static str,
    pub source_status_approved: &'static str,
    pub source_status_grandfathered: &'static str,
    pub source_status_redefined: &'static str,
    pub source_status_renamed: &'static str,
    pub source_status_uncertain: &'static str,
    pub source_status_questionable: &'static str,
    pub source_status_discredited: &'static str,
    pub source_status_unknown: &'static str,
    pub discovery_country: &'static str,
    pub official_identity_coverage_note: &'static str,
    pub published_references: &'static str,
    pub source_and_license: &'static str,
    pub formula: &'static str,
    pub evidence: &'static str,
    pub supports: &'static str,
    pub source_license: &'static str,
    pub attribution: &'static str,
    pub attribution_party: &'static str,
    pub source_work: &'static str,
    pub license_terms: &'static str,
    pub changes_made: &'static str,
    pub no_endorsement: &'static str,
    pub derived_data_license: &'static str,
    pub availability: &'static str,
    pub detail_suffix: &'static str,
    pub identity: &'static str,
    pub family: &'static str,
    pub identifiers: &'static str,
    pub properties: &'static str,
    pub safety: &'static str,
    pub status: &'static str,
    pub status_note: &'static str,
    pub status_preliminary: &'static str,
    pub status_sourced: &'static str,
    pub status_reviewed: &'static str,
    pub status_verified: &'static str,
    pub status_disputed: &'static str,
    pub evidence_empty: &'static str,
    pub source_retrieved: &'static str,
    pub open_source: &'static str,
    pub buy_heading: &'static str,
    pub buy_intro: &'static str,
    pub no_offers_title: &'static str,
    pub no_offers_body: &'static str,
    pub price_on_request: &'static str,
    pub price_unit: &'static str,
    pub price_lot: &'static str,
    pub price_package: &'static str,
    pub minimum_order: &'static str,
    pub not_specified: &'static str,
    pub purity: &'static str,
    pub grade: &'static str,
    pub origin: &'static str,
    pub last_checked: &'static str,
    pub open_provider: &'static str,
    pub stock_in: &'static str,
    pub stock_limited: &'static str,
    pub stock_made_to_order: &'static str,
    pub stock_quote: &'static str,
    pub stock_out: &'static str,
    pub stock_unknown: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct ReviewText {
    pub queue_link: &'static str,
    pub title: &'static str,
    pub subtitle: &'static str,
    pub back_to_admin: &'static str,
    pub empty: &'static str,
    pub pending_candidates: &'static str,
    pub review_id: &'static str,
    pub revision: &'static str,
    pub creates_new: &'static str,
    pub updates_existing: &'static str,
    pub view_current: &'static str,
    pub submitted_source: &'static str,
    pub submitted_at: &'static str,
    pub quality: &'static str,
    pub slug: &'static str,
    pub cas_number: &'static str,
    pub synonyms: &'static str,
    pub record_license: &'static str,
    pub publisher: &'static str,
    pub claim_scope: &'static str,
    pub claim_value: &'static str,
    pub claim_locator: &'static str,
    pub claim_note: &'static str,
    pub claim_details: &'static str,
    pub confidence: &'static str,
    pub evidence_state: &'static str,
    pub retrieved_at: &'static str,
    pub content_hash: &'static str,
    pub complete_payload: &'static str,
    pub complete_payload_hint: &'static str,
    pub operator_note: &'static str,
    pub operator_note_placeholder: &'static str,
    pub note_required: &'static str,
    pub approve: &'static str,
    pub reject: &'static str,
    pub approved_notice: &'static str,
    pub rejected_notice: &'static str,
    pub decision_warning: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct IngestionText {
    pub link: &'static str,
    pub title: &'static str,
    pub subtitle: &'static str,
    pub back_to_admin: &'static str,
    pub individual_review_link: &'static str,
    pub create_title: &'static str,
    pub create_hint: &'static str,
    pub manifest_payload: &'static str,
    pub dataset: &'static str,
    pub source_name: &'static str,
    pub source_url: &'static str,
    pub attribution_review: &'static str,
    pub historical_attribution_missing: &'static str,
    pub release_version: &'static str,
    pub released_at: &'static str,
    pub retrieved_at: &'static str,
    pub manifest_hash: &'static str,
    pub artifact_hash: &'static str,
    pub parser_name: &'static str,
    pub parser_version: &'static str,
    pub parser_revision: &'static str,
    pub parser_configuration_hash: &'static str,
    pub record_license: &'static str,
    pub expected_chunks: &'static str,
    pub expected_records: &'static str,
    pub create_batch: &'static str,
    pub upload_title: &'static str,
    pub upload_hint: &'static str,
    pub batch_id: &'static str,
    pub chunk_number: &'static str,
    pub chunk_payload: &'static str,
    pub upload_chunk: &'static str,
    pub finalize_title: &'static str,
    pub finalize_hint: &'static str,
    pub finalize_validate: &'static str,
    pub batches_title: &'static str,
    pub empty: &'static str,
    pub status: &'static str,
    pub status_receiving: &'static str,
    pub status_ready: &'static str,
    pub status_needs_attention: &'static str,
    pub status_approved: &'static str,
    pub status_rejected: &'static str,
    pub progress: &'static str,
    pub created_at: &'static str,
    pub finalized_at: &'static str,
    pub uploaded_chunks: &'static str,
    pub records: &'static str,
    pub created: &'static str,
    pub adopted: &'static str,
    pub updated: &'static str,
    pub unchanged: &'static str,
    pub missing: &'static str,
    pub blockers: &'static str,
    pub identity_warnings: &'static str,
    pub anomaly_samples: &'static str,
    pub no_anomalies: &'static str,
    pub review_samples: &'static str,
    pub source_record: &'static str,
    pub nomenclature_status: &'static str,
    pub valid_species: &'static str,
    pub yes: &'static str,
    pub no: &'static str,
    pub report_hash: &'static str,
    pub base_batch: &'static str,
    pub decision_title: &'static str,
    pub decision_warning: &'static str,
    pub acknowledge_warning: &'static str,
    pub confirmation: &'static str,
    pub confirmation_hint: &'static str,
    pub operator_note: &'static str,
    pub operator_note_placeholder: &'static str,
    pub approve_release: &'static str,
    pub reject_release: &'static str,
    pub working: &'static str,
    pub request_failed: &'static str,
    pub batch_created: &'static str,
    pub chunk_uploaded: &'static str,
    pub validation_complete: &'static str,
    pub approved_notice: &'static str,
    pub rejected_notice: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct UiText {
    pub registry: RegistryText,
    pub review: ReviewText,
    pub ingestion: IngestionText,
    pub nav_home: &'static str,
    pub nav_all_minerals: &'static str,
    pub nav_about: &'static str,
    pub nav_admin: &'static str,
    pub nav_login: &'static str,
    pub nav_current_mineral: &'static str,
    pub nav_report: &'static str,
    pub session_admin_active: &'static str,
    pub session_public_mode: &'static str,
    pub session_secure_active: &'static str,
    pub session_auth_required: &'static str,

    pub home_title: &'static str,
    pub home_subtitle: &'static str,
    pub home_select_language: &'static str,
    pub home_continue: &'static str,

    pub catalog_title: &'static str,
    pub catalog_subtitle: &'static str,
    pub no_minerals: &'static str,
    pub open_mineral: &'static str,
    pub all_minerals_title: &'static str,
    pub all_minerals_subtitle: &'static str,
    pub all_minerals_published_label: &'static str,
    pub all_minerals_estimated_label: &'static str,
    pub all_minerals_disclaimer: &'static str,

    pub label_family: &'static str,
    pub label_formula: &'static str,
    pub label_hardness: &'static str,
    pub label_density: &'static str,
    pub label_description: &'static str,
    pub label_crystal_system: &'static str,
    pub label_color: &'static str,
    pub label_streak: &'static str,
    pub label_luster: &'static str,
    pub label_notes: &'static str,
    pub label_hardness_band: &'static str,
    pub label_density_band: &'static str,
    pub label_dominant_element: &'static str,
    pub label_audience: &'static str,
    pub label_purpose: &'static str,
    pub label_site_context: &'static str,
    pub label_generated_utc: &'static str,
    pub label_weight_pct: &'static str,

    pub mineral_profile: &'static str,
    pub major_composition: &'static str,
    pub computed_classification: &'static str,
    pub report_builder: &'static str,
    pub report_builder_subtitle: &'static str,
    pub generate_pdf: &'static str,
    pub status_pdf: &'static str,
    pub status_html: &'static str,
    pub status_pdf_failed: &'static str,
    pub current_chain_output: &'static str,
    pub recommendations_heading: &'static str,

    pub about_title: &'static str,
    pub about_subtitle: &'static str,
    pub about_operating_model: &'static str,
    pub about_operating_body: &'static str,
    pub about_path_note: &'static str,

    pub footer_contact: &'static str,
    pub footer_legal: &'static str,
    pub footer_mission: &'static str,
    pub footer_contact_us: &'static str,
    pub footer_support: &'static str,
    pub footer_work_with_us: &'static str,
    pub footer_account: &'static str,
    pub footer_legal_link: &'static str,
    pub footer_privacy_policy: &'static str,
    pub footer_terms_of_service: &'static str,
    pub footer_returns_and_refunds: &'static str,
    pub footer_shipping: &'static str,
    pub footer_about_us: &'static str,
    pub footer_conflict_free_minerals: &'static str,
    pub footer_faq: &'static str,
    pub footer_powered_trust_by: &'static str,

    pub report_title_suffix: &'static str,
    pub context_heading: &'static str,
    pub snapshot_heading: &'static str,
    pub summary_heading: &'static str,
    pub major_elements_heading: &'static str,
    pub notes_heading: &'static str,
}

fn registry_en() -> RegistryText {
    RegistryText {
        theme_toggle: "Toggle dark mode",
        eyebrow: "Waajacu mineral discovery",
        title: "Global Mineral Registry",
        subtitle: "Find minerals, follow their scientific sources, and discover current ways to obtain them.",
        source_links: "Scientific sources",
        buying_options: "Buying options",
        search_label: "Search by name, formula, identifier, or synonym",
        search_placeholder: "quartz, SiO₂, amethyst",
        search_action: "Search",
        empty_results: "No matching minerals were found. Try another name, formula, or identifier.",
        pagination_showing: "Showing",
        pagination_of: "of",
        pagination_results: "results",
        pagination_page: "Page",
        pagination_previous: "Previous",
        pagination_next: "Next",
        kind_mineral: "Mineral",
        ima_number: "IMA number",
        ima_symbol: "IMA symbol",
        mineral_species: "Mineral species",
        nomenclature_ima_approved: "IMA approved",
        nomenclature_recognized: "Recognized (grandfathered)",
        nomenclature_redefined: "Redefined",
        nomenclature_renamed: "Renamed",
        nomenclature_uncertain: "Approval uncertain",
        nomenclature_questionable: "Questionable status",
        nomenclature_discredited: "Discredited",
        nomenclature_unknown: "Status not classified",
        valid_mineral_species: "Valid mineral species",
        mineral_facts: "Mineral facts",
        nomenclature: "Nomenclature",
        source_status_approved: "Approved by the IMA",
        source_status_grandfathered: "Accepted established species",
        source_status_redefined: "Definition revised by the IMA",
        source_status_renamed: "Officially renamed",
        source_status_uncertain: "IMA approval is uncertain",
        source_status_questionable: "Status considered questionable",
        source_status_discredited: "No longer recognized as a valid species",
        source_status_unknown: "Official status not classified",
        discovery_country: "Discovery country",
        official_identity_coverage_note: "This entry currently documents the mineral's official identity and nomenclature. Its occurrence, crystal structure, physical and optical properties have not yet been added.",
        published_references: "Published references",
        source_and_license: "Source and license",
        formula: "Formula",
        evidence: "Evidence",
        supports: "Supports",
        source_license: "Source license",
        attribution: "Source attribution",
        attribution_party: "Credit",
        source_work: "Source work",
        license_terms: "License terms",
        changes_made: "Changes made",
        no_endorsement: "No endorsement",
        derived_data_license: "Derived data license",
        availability: "Availability",
        detail_suffix: "Mineral record",
        identity: "Identity",
        family: "Family",
        identifiers: "Identifiers",
        properties: "Properties",
        safety: "Safety",
        status: "Knowledge status",
        status_note: "The status reflects the review completed so far. Scientific sources and seller listings are assessed separately.",
        status_preliminary: "Preliminary",
        status_sourced: "Sources attached",
        status_reviewed: "Reviewed",
        status_verified: "Verified",
        status_disputed: "Under review",
        evidence_empty: "No scientific source is attached yet. Use this record as a starting point and verify it independently.",
        source_retrieved: "Retrieved",
        open_source: "Read source",
        buy_heading: "Buy or source",
        buy_intro: "Compare available supplier listings. Confirm specifications, documentation, and shipping terms with the supplier before purchasing.",
        no_offers_title: "No current buying options",
        no_offers_body: "We have not found a current supplier listing for this mineral yet.",
        price_on_request: "Price on request",
        price_unit: "unit",
        price_lot: "lot",
        price_package: "package",
        minimum_order: "Minimum order",
        not_specified: "Not specified",
        purity: "Purity",
        grade: "Grade",
        origin: "Origin",
        last_checked: "Last checked",
        open_provider: "View supplier",
        stock_in: "In stock",
        stock_limited: "Limited availability",
        stock_made_to_order: "Made to order",
        stock_quote: "Contact for availability",
        stock_out: "Out of stock",
        stock_unknown: "Availability not confirmed",
    }
}

fn registry_es() -> RegistryText {
    RegistryText {
        theme_toggle: "Cambiar modo oscuro",
        eyebrow: "Descubrimiento de minerales Waajacu",
        title: "Registro mundial de minerales",
        subtitle: "Encuentra minerales, consulta sus fuentes científicas y descubre formas actuales de obtenerlos.",
        source_links: "Fuentes científicas",
        buying_options: "Opciones de compra",
        search_label: "Buscar por nombre, fórmula, identificador o sinónimo",
        search_placeholder: "cuarzo, SiO₂, amatista",
        search_action: "Buscar",
        empty_results: "No encontramos minerales coincidentes. Prueba con otro nombre, fórmula o identificador.",
        pagination_showing: "Mostrando",
        pagination_of: "de",
        pagination_results: "resultados",
        pagination_page: "Página",
        pagination_previous: "Anterior",
        pagination_next: "Siguiente",
        kind_mineral: "Mineral",
        ima_number: "Número IMA",
        ima_symbol: "Símbolo IMA",
        mineral_species: "Especie mineral",
        nomenclature_ima_approved: "Aprobado por la IMA",
        nomenclature_recognized: "Reconocido (preexistente)",
        nomenclature_redefined: "Redefinido",
        nomenclature_renamed: "Renombrado",
        nomenclature_uncertain: "Aprobación incierta",
        nomenclature_questionable: "Estado cuestionable",
        nomenclature_discredited: "Desacreditado",
        nomenclature_unknown: "Estado sin clasificar",
        valid_mineral_species: "Especie mineral válida",
        mineral_facts: "Datos del mineral",
        nomenclature: "Nomenclatura",
        source_status_approved: "Aprobado por la IMA",
        source_status_grandfathered: "Especie establecida y aceptada",
        source_status_redefined: "Definición revisada por la IMA",
        source_status_renamed: "Renombrado oficialmente",
        source_status_uncertain: "La aprobación de la IMA es incierta",
        source_status_questionable: "El estado se considera cuestionable",
        source_status_discredited: "Ya no se reconoce como especie válida",
        source_status_unknown: "Estado oficial sin clasificar",
        discovery_country: "País de descubrimiento",
        official_identity_coverage_note: "Esta ficha documenta actualmente la identidad y la nomenclatura oficiales del mineral. Aún no se han añadido su presencia geológica, estructura cristalina ni propiedades físicas y ópticas.",
        published_references: "Referencias publicadas",
        source_and_license: "Fuente y licencia",
        formula: "Fórmula",
        evidence: "Evidencia",
        supports: "Respalda",
        source_license: "Licencia de la fuente",
        attribution: "Atribución de la fuente",
        attribution_party: "Crédito",
        source_work: "Obra fuente",
        license_terms: "Condiciones de licencia",
        changes_made: "Cambios realizados",
        no_endorsement: "Sin respaldo",
        derived_data_license: "Licencia de los datos derivados",
        availability: "Disponibilidad",
        detail_suffix: "Ficha del mineral",
        identity: "Identidad",
        family: "Familia",
        identifiers: "Identificadores",
        properties: "Propiedades",
        safety: "Seguridad",
        status: "Estado del conocimiento",
        status_note: "El estado refleja la revisión realizada hasta ahora. Las fuentes científicas y las ofertas comerciales se evalúan por separado.",
        status_preliminary: "Preliminar",
        status_sourced: "Con fuentes",
        status_reviewed: "Revisado",
        status_verified: "Verificado",
        status_disputed: "En revisión",
        evidence_empty: "Aún no hay una fuente científica asociada. Usa esta ficha como punto de partida y verifícala de forma independiente.",
        source_retrieved: "Consultada",
        open_source: "Leer fuente",
        buy_heading: "Comprar o conseguir",
        buy_intro: "Compara ofertas disponibles. Confirma las especificaciones, la documentación y el envío con el proveedor antes de comprar.",
        no_offers_title: "Sin opciones de compra actuales",
        no_offers_body: "Todavía no hemos encontrado una oferta vigente para este mineral.",
        price_on_request: "Precio bajo consulta",
        price_unit: "unidad",
        price_lot: "lote",
        price_package: "paquete",
        minimum_order: "Pedido mínimo",
        not_specified: "No especificado",
        purity: "Pureza",
        grade: "Grado",
        origin: "Origen",
        last_checked: "Última comprobación",
        open_provider: "Ver proveedor",
        stock_in: "En stock",
        stock_limited: "Disponibilidad limitada",
        stock_made_to_order: "Fabricado por encargo",
        stock_quote: "Consultar disponibilidad",
        stock_out: "Agotado",
        stock_unknown: "Disponibilidad sin confirmar",
    }
}

fn registry_cs() -> RegistryText {
    RegistryText {
        theme_toggle: "Přepnout tmavý režim",
        eyebrow: "Vyhledávání minerálů Waajacu",
        title: "Globální registr minerálů",
        subtitle: "Hledejte minerály, ověřujte jejich vědecké zdroje a zjistěte, kde je lze získat.",
        source_links: "Vědecké zdroje",
        buying_options: "Možnosti nákupu",
        search_label: "Hledat podle názvu, vzorce, identifikátoru nebo synonyma",
        search_placeholder: "křemen, SiO₂, ametyst",
        search_action: "Hledat",
        empty_results: "Nebyly nalezeny žádné odpovídající minerály. Zkuste jiný název, vzorec nebo identifikátor.",
        pagination_showing: "Zobrazeno",
        pagination_of: "z",
        pagination_results: "výsledků",
        pagination_page: "Stránka",
        pagination_previous: "Předchozí",
        pagination_next: "Další",
        kind_mineral: "Minerál",
        ima_number: "Číslo IMA",
        ima_symbol: "Symbol IMA",
        mineral_species: "Minerální druh",
        nomenclature_ima_approved: "Schváleno IMA",
        nomenclature_recognized: "Uznaný (historický)",
        nomenclature_redefined: "Redefinováno",
        nomenclature_renamed: "Přejmenováno",
        nomenclature_uncertain: "Schválení nejisté",
        nomenclature_questionable: "Sporný stav",
        nomenclature_discredited: "Zneplatněno",
        nomenclature_unknown: "Stav není klasifikován",
        valid_mineral_species: "Platný minerální druh",
        mineral_facts: "Údaje o minerálu",
        nomenclature: "Nomenklatura",
        source_status_approved: "Schváleno komisí IMA",
        source_status_grandfathered: "Přijatý zavedený druh",
        source_status_redefined: "Definice revidovaná komisí IMA",
        source_status_renamed: "Oficiálně přejmenováno",
        source_status_uncertain: "Schválení IMA je nejisté",
        source_status_questionable: "Stav je považován za sporný",
        source_status_discredited: "Již není uznáváno jako platný druh",
        source_status_unknown: "Oficiální stav není klasifikován",
        discovery_country: "Země objevu",
        official_identity_coverage_note: "Tento záznam zatím dokumentuje oficiální identitu a nomenklaturu minerálu. Údaje o výskytu, krystalové struktuře a fyzikálních a optických vlastnostech dosud nebyly doplněny.",
        published_references: "Publikované prameny",
        source_and_license: "Zdroj a licence",
        formula: "Vzorec",
        evidence: "Podklady",
        supports: "Dokládá",
        source_license: "Licence zdroje",
        attribution: "Uvedení zdroje",
        attribution_party: "Uvedení autora",
        source_work: "Zdrojové dílo",
        license_terms: "Licenční podmínky",
        changes_made: "Provedené změny",
        no_endorsement: "Bez podpory",
        derived_data_license: "Licence odvozených dat",
        availability: "Dostupnost",
        detail_suffix: "Záznam minerálu",
        identity: "Identita",
        family: "Skupina",
        identifiers: "Identifikátory",
        properties: "Vlastnosti",
        safety: "Bezpečnost",
        status: "Stav poznání",
        status_note: "Stav vyjadřuje dosavadní úroveň kontroly. Vědecké zdroje a nabídky prodejců se posuzují odděleně.",
        status_preliminary: "Předběžné",
        status_sourced: "Doplněné zdroje",
        status_reviewed: "Zkontrolováno",
        status_verified: "Ověřeno",
        status_disputed: "V přezkumu",
        evidence_empty: "Zatím není připojen žádný vědecký zdroj. Použijte záznam jako výchozí bod a nezávisle jej ověřte.",
        source_retrieved: "Získáno",
        open_source: "Otevřít zdroj",
        buy_heading: "Koupit nebo získat",
        buy_intro: "Porovnejte dostupné nabídky. Před nákupem si u dodavatele ověřte specifikace, dokumentaci a podmínky dopravy.",
        no_offers_title: "Žádné aktuální možnosti nákupu",
        no_offers_body: "Pro tento minerál jsme zatím nenašli aktuální nabídku dodavatele.",
        price_on_request: "Cena na vyžádání",
        price_unit: "jednotka",
        price_lot: "šarže",
        price_package: "balení",
        minimum_order: "Minimální objednávka",
        not_specified: "Neuvedeno",
        purity: "Čistota",
        grade: "Jakost",
        origin: "Původ",
        last_checked: "Naposledy ověřeno",
        open_provider: "Zobrazit dodavatele",
        stock_in: "Skladem",
        stock_limited: "Omezená dostupnost",
        stock_made_to_order: "Na objednávku",
        stock_quote: "Dostupnost na dotaz",
        stock_out: "Není skladem",
        stock_unknown: "Dostupnost nepotvrzena",
    }
}

fn registry_de() -> RegistryText {
    RegistryText {
        theme_toggle: "Dunkelmodus umschalten",
        eyebrow: "Waajacu Mineralsuche",
        title: "Globales Mineralregister",
        subtitle: "Mineralien finden, wissenschaftliche Quellen nachvollziehen und aktuelle Bezugswege entdecken.",
        source_links: "Wissenschaftliche Quellen",
        buying_options: "Bezugsoptionen",
        search_label: "Nach Name, Formel, Kennung oder Synonym suchen",
        search_placeholder: "Quarz, SiO₂, Amethyst",
        search_action: "Suchen",
        empty_results: "Keine passenden Mineralien gefunden. Versuchen Sie einen anderen Namen, eine Formel oder Kennung.",
        pagination_showing: "Anzeige",
        pagination_of: "von",
        pagination_results: "Ergebnissen",
        pagination_page: "Seite",
        pagination_previous: "Zurück",
        pagination_next: "Weiter",
        kind_mineral: "Mineral",
        ima_number: "IMA-Nummer",
        ima_symbol: "IMA-Symbol",
        mineral_species: "Mineralart",
        nomenclature_ima_approved: "Von der IMA anerkannt",
        nomenclature_recognized: "Anerkannt (historisch)",
        nomenclature_redefined: "Neu definiert",
        nomenclature_renamed: "Umbenannt",
        nomenclature_uncertain: "Anerkennung unsicher",
        nomenclature_questionable: "Fraglicher Status",
        nomenclature_discredited: "Diskreditiert",
        nomenclature_unknown: "Status nicht klassifiziert",
        valid_mineral_species: "Gültige Mineralart",
        mineral_facts: "Mineraldaten",
        nomenclature: "Nomenklatur",
        source_status_approved: "Von der IMA anerkannt",
        source_status_grandfathered: "Anerkannte etablierte Mineralart",
        source_status_redefined: "Definition von der IMA überarbeitet",
        source_status_renamed: "Offiziell umbenannt",
        source_status_uncertain: "Die Anerkennung durch die IMA ist unsicher",
        source_status_questionable: "Der Status gilt als fraglich",
        source_status_discredited: "Nicht mehr als gültige Mineralart anerkannt",
        source_status_unknown: "Offizieller Status nicht klassifiziert",
        discovery_country: "Entdeckungsland",
        official_identity_coverage_note: "Dieser Eintrag dokumentiert derzeit die offizielle Identität und Nomenklatur des Minerals. Vorkommen, Kristallstruktur sowie physikalische und optische Eigenschaften wurden noch nicht ergänzt.",
        published_references: "Veröffentlichte Literatur",
        source_and_license: "Quelle und Lizenz",
        formula: "Formel",
        evidence: "Quellenlage",
        supports: "Belegt",
        source_license: "Quellenlizenz",
        attribution: "Quellenangabe",
        attribution_party: "Urheberangabe",
        source_work: "Quellenwerk",
        license_terms: "Lizenzbedingungen",
        changes_made: "Vorgenommene Änderungen",
        no_endorsement: "Keine Unterstützung",
        derived_data_license: "Lizenz der abgeleiteten Daten",
        availability: "Verfügbarkeit",
        detail_suffix: "Mineraleintrag",
        identity: "Identität",
        family: "Familie",
        identifiers: "Kennungen",
        properties: "Eigenschaften",
        safety: "Sicherheit",
        status: "Wissensstand",
        status_note: "Der Status zeigt den bisherigen Prüfstand. Wissenschaftliche Quellen und Händlerangebote werden getrennt bewertet.",
        status_preliminary: "Vorläufig",
        status_sourced: "Quellen vorhanden",
        status_reviewed: "Geprüft",
        status_verified: "Verifiziert",
        status_disputed: "In Klärung",
        evidence_empty: "Noch ist keine wissenschaftliche Quelle hinterlegt. Nutzen Sie den Eintrag als Ausgangspunkt und prüfen Sie ihn unabhängig.",
        source_retrieved: "Abgerufen",
        open_source: "Quelle öffnen",
        buy_heading: "Kaufen oder beschaffen",
        buy_intro: "Vergleichen Sie verfügbare Angebote. Klären Sie Spezifikationen, Dokumentation und Versand vor dem Kauf direkt mit dem Anbieter.",
        no_offers_title: "Derzeit keine Bezugsoptionen",
        no_offers_body: "Für dieses Mineral wurde noch kein aktuelles Anbieterangebot gefunden.",
        price_on_request: "Preis auf Anfrage",
        price_unit: "Einheit",
        price_lot: "Los",
        price_package: "Packung",
        minimum_order: "Mindestbestellung",
        not_specified: "Nicht angegeben",
        purity: "Reinheit",
        grade: "Qualität",
        origin: "Herkunft",
        last_checked: "Zuletzt geprüft",
        open_provider: "Anbieter öffnen",
        stock_in: "Auf Lager",
        stock_limited: "Begrenzt verfügbar",
        stock_made_to_order: "Auf Bestellung",
        stock_quote: "Verfügbarkeit anfragen",
        stock_out: "Nicht auf Lager",
        stock_unknown: "Verfügbarkeit unbestätigt",
    }
}

fn registry_fr() -> RegistryText {
    RegistryText {
        theme_toggle: "Changer le mode sombre",
        eyebrow: "Découverte de minéraux Waajacu",
        title: "Registre mondial des minéraux",
        subtitle: "Trouvez des minéraux, consultez leurs sources scientifiques et découvrez comment vous les procurer.",
        source_links: "Sources scientifiques",
        buying_options: "Options d’achat",
        search_label: "Rechercher par nom, formule, identifiant ou synonyme",
        search_placeholder: "quartz, SiO₂, améthyste",
        search_action: "Rechercher",
        empty_results: "Aucun minéral correspondant. Essayez un autre nom, une formule ou un identifiant.",
        pagination_showing: "Affichage",
        pagination_of: "sur",
        pagination_results: "résultats",
        pagination_page: "Page",
        pagination_previous: "Précédente",
        pagination_next: "Suivante",
        kind_mineral: "Minéral",
        ima_number: "Numéro IMA",
        ima_symbol: "Symbole IMA",
        mineral_species: "Espèce minérale",
        nomenclature_ima_approved: "Approuvée par l’IMA",
        nomenclature_recognized: "Reconnue (historique)",
        nomenclature_redefined: "Redéfinie",
        nomenclature_renamed: "Renommée",
        nomenclature_uncertain: "Approbation incertaine",
        nomenclature_questionable: "Statut douteux",
        nomenclature_discredited: "Discréditée",
        nomenclature_unknown: "Statut non classé",
        valid_mineral_species: "Espèce minérale valide",
        mineral_facts: "Données minéralogiques",
        nomenclature: "Nomenclature",
        source_status_approved: "Approuvée par l’IMA",
        source_status_grandfathered: "Espèce établie et reconnue",
        source_status_redefined: "Définition révisée par l’IMA",
        source_status_renamed: "Officiellement renommée",
        source_status_uncertain: "L’approbation par l’IMA est incertaine",
        source_status_questionable: "Le statut est considéré comme douteux",
        source_status_discredited: "N’est plus reconnue comme espèce valide",
        source_status_unknown: "Statut officiel non classé",
        discovery_country: "Pays de découverte",
        official_identity_coverage_note: "Cette fiche documente actuellement l’identité et la nomenclature officielles du minéral. Son occurrence, sa structure cristalline et ses propriétés physiques et optiques n’ont pas encore été ajoutées.",
        published_references: "Références publiées",
        source_and_license: "Source et licence",
        formula: "Formule",
        evidence: "Sources",
        supports: "Étaye",
        source_license: "Licence de la source",
        attribution: "Attribution de la source",
        attribution_party: "Crédit",
        source_work: "Œuvre source",
        license_terms: "Conditions de licence",
        changes_made: "Modifications apportées",
        no_endorsement: "Aucune approbation",
        derived_data_license: "Licence des données dérivées",
        availability: "Disponibilité",
        detail_suffix: "Fiche minérale",
        identity: "Identité",
        family: "Famille",
        identifiers: "Identifiants",
        properties: "Propriétés",
        safety: "Sécurité",
        status: "État des connaissances",
        status_note: "Le statut reflète le niveau de contrôle réalisé. Les sources scientifiques et les offres commerciales sont évaluées séparément.",
        status_preliminary: "Préliminaire",
        status_sourced: "Sources disponibles",
        status_reviewed: "Relu",
        status_verified: "Vérifié",
        status_disputed: "En cours d’examen",
        evidence_empty: "Aucune source scientifique n’est encore associée. Utilisez cette fiche comme point de départ et vérifiez-la indépendamment.",
        source_retrieved: "Consultée le",
        open_source: "Lire la source",
        buy_heading: "Acheter ou se procurer",
        buy_intro: "Comparez les offres disponibles. Confirmez les spécifications, les documents et la livraison auprès du fournisseur avant tout achat.",
        no_offers_title: "Aucune option d’achat actuelle",
        no_offers_body: "Nous n’avons pas encore trouvé d’offre fournisseur actuelle pour ce minéral.",
        price_on_request: "Prix sur demande",
        price_unit: "unité",
        price_lot: "lot",
        price_package: "conditionnement",
        minimum_order: "Commande minimale",
        not_specified: "Non précisé",
        purity: "Pureté",
        grade: "Qualité",
        origin: "Origine",
        last_checked: "Dernière vérification",
        open_provider: "Voir le fournisseur",
        stock_in: "En stock",
        stock_limited: "Disponibilité limitée",
        stock_made_to_order: "Fabriqué sur commande",
        stock_quote: "Disponibilité sur demande",
        stock_out: "Rupture de stock",
        stock_unknown: "Disponibilité non confirmée",
    }
}

fn registry_pt() -> RegistryText {
    RegistryText {
        theme_toggle: "Alternar modo escuro",
        eyebrow: "Descoberta de minerais Waajacu",
        title: "Registro mundial de minerais",
        subtitle: "Encontre minerais, consulte suas fontes científicas e descubra formas atuais de obtê-los.",
        source_links: "Fontes científicas",
        buying_options: "Opções de compra",
        search_label: "Buscar por nome, fórmula, identificador ou sinônimo",
        search_placeholder: "quartzo, SiO₂, ametista",
        search_action: "Buscar",
        empty_results: "Nenhum mineral correspondente foi encontrado. Tente outro nome, fórmula ou identificador.",
        pagination_showing: "A mostrar",
        pagination_of: "de",
        pagination_results: "resultados",
        pagination_page: "Página",
        pagination_previous: "Anterior",
        pagination_next: "Seguinte",
        kind_mineral: "Mineral",
        ima_number: "Número IMA",
        ima_symbol: "Símbolo IMA",
        mineral_species: "Espécie mineral",
        nomenclature_ima_approved: "Aprovado pela IMA",
        nomenclature_recognized: "Reconhecido (histórico)",
        nomenclature_redefined: "Redefinido",
        nomenclature_renamed: "Renomeado",
        nomenclature_uncertain: "Aprovação incerta",
        nomenclature_questionable: "Status questionável",
        nomenclature_discredited: "Desacreditado",
        nomenclature_unknown: "Status não classificado",
        valid_mineral_species: "Espécie mineral válida",
        mineral_facts: "Dados do mineral",
        nomenclature: "Nomenclatura",
        source_status_approved: "Aprovado pela IMA",
        source_status_grandfathered: "Espécie estabelecida e aceita",
        source_status_redefined: "Definição revista pela IMA",
        source_status_renamed: "Renomeado oficialmente",
        source_status_uncertain: "A aprovação da IMA é incerta",
        source_status_questionable: "O status é considerado questionável",
        source_status_discredited: "Já não é reconhecido como espécie válida",
        source_status_unknown: "Status oficial não classificado",
        discovery_country: "País de descoberta",
        official_identity_coverage_note: "Esta ficha documenta atualmente a identidade e a nomenclatura oficiais do mineral. Sua ocorrência, estrutura cristalina e propriedades físicas e ópticas ainda não foram adicionadas.",
        published_references: "Referências publicadas",
        source_and_license: "Fonte e licença",
        formula: "Fórmula",
        evidence: "Evidências",
        supports: "Sustenta",
        source_license: "Licença da fonte",
        attribution: "Atribuição da fonte",
        attribution_party: "Crédito",
        source_work: "Obra de origem",
        license_terms: "Termos da licença",
        changes_made: "Alterações realizadas",
        no_endorsement: "Sem endosso",
        derived_data_license: "Licença dos dados derivados",
        availability: "Disponibilidade",
        detail_suffix: "Ficha do mineral",
        identity: "Identidade",
        family: "Família",
        identifiers: "Identificadores",
        properties: "Propriedades",
        safety: "Segurança",
        status: "Estado do conhecimento",
        status_note: "O status reflete a revisão concluída até agora. Fontes científicas e ofertas comerciais são avaliadas separadamente.",
        status_preliminary: "Preliminar",
        status_sourced: "Com fontes",
        status_reviewed: "Revisado",
        status_verified: "Verificado",
        status_disputed: "Em revisão",
        evidence_empty: "Ainda não há uma fonte científica associada. Use esta ficha como ponto de partida e faça uma verificação independente.",
        source_retrieved: "Consultada em",
        open_source: "Ler fonte",
        buy_heading: "Comprar ou obter",
        buy_intro: "Compare as ofertas disponíveis. Confirme especificações, documentação e envio com o fornecedor antes de comprar.",
        no_offers_title: "Sem opções de compra atuais",
        no_offers_body: "Ainda não encontramos uma oferta atual de fornecedor para este mineral.",
        price_on_request: "Preço sob consulta",
        price_unit: "unidade",
        price_lot: "lote",
        price_package: "embalagem",
        minimum_order: "Pedido mínimo",
        not_specified: "Não especificado",
        purity: "Pureza",
        grade: "Grau",
        origin: "Origem",
        last_checked: "Última verificação",
        open_provider: "Ver fornecedor",
        stock_in: "Em estoque",
        stock_limited: "Disponibilidade limitada",
        stock_made_to_order: "Feito sob encomenda",
        stock_quote: "Consultar disponibilidade",
        stock_out: "Fora de estoque",
        stock_unknown: "Disponibilidade não confirmada",
    }
}

fn registry_zh() -> RegistryText {
    RegistryText {
        theme_toggle: "切换深色模式",
        eyebrow: "Waajacu 矿物发现",
        title: "全球矿物名录",
        subtitle: "查找矿物，追溯科学来源，并了解当前的获取渠道。",
        source_links: "科学来源",
        buying_options: "购买选项",
        search_label: "按名称、化学式、标识符或别名搜索",
        search_placeholder: "石英、SiO₂、紫水晶",
        search_action: "搜索",
        empty_results: "未找到匹配的矿物。请尝试其他名称、化学式或标识符。",
        pagination_showing: "显示",
        pagination_of: "共",
        pagination_results: "个结果",
        pagination_page: "页",
        pagination_previous: "上一页",
        pagination_next: "下一页",
        kind_mineral: "矿物",
        ima_number: "IMA 编号",
        ima_symbol: "IMA 符号",
        mineral_species: "矿物种",
        nomenclature_ima_approved: "IMA 已批准",
        nomenclature_recognized: "已认可（历史矿物种）",
        nomenclature_redefined: "已重新定义",
        nomenclature_renamed: "已更名",
        nomenclature_uncertain: "批准状态不确定",
        nomenclature_questionable: "状态存疑",
        nomenclature_discredited: "已撤销认可",
        nomenclature_unknown: "状态未分类",
        valid_mineral_species: "有效矿物种",
        mineral_facts: "矿物资料",
        nomenclature: "命名状态",
        source_status_approved: "经 IMA 批准",
        source_status_grandfathered: "已接受的既有矿物种",
        source_status_redefined: "定义已由 IMA 修订",
        source_status_renamed: "已正式更名",
        source_status_uncertain: "IMA 批准情况不确定",
        source_status_questionable: "该状态被视为存疑",
        source_status_discredited: "不再被认可为有效矿物种",
        source_status_unknown: "官方状态未分类",
        discovery_country: "发现国家",
        official_identity_coverage_note:
            "本条目目前记录该矿物的官方身份与命名状态。其产状、晶体结构以及物理和光学性质尚未补充。",
        published_references: "已发表参考文献",
        source_and_license: "来源与许可",
        formula: "化学式",
        evidence: "科学依据",
        supports: "支持内容",
        source_license: "来源许可",
        attribution: "来源署名",
        attribution_party: "署名",
        source_work: "来源作品",
        license_terms: "许可条款",
        changes_made: "所作更改",
        no_endorsement: "不代表认可",
        derived_data_license: "衍生数据许可",
        availability: "供应情况",
        detail_suffix: "矿物档案",
        identity: "基本信息",
        family: "类别",
        identifiers: "标识符",
        properties: "性质",
        safety: "安全",
        status: "知识状态",
        status_note: "该状态反映目前已完成的审核程度。科学来源与供应商信息分别评估。",
        status_preliminary: "初步",
        status_sourced: "已有来源",
        status_reviewed: "已审核",
        status_verified: "已核实",
        status_disputed: "审核中",
        evidence_empty: "尚未关联科学来源。请将本档案作为研究起点，并进行独立核实。",
        source_retrieved: "查阅日期",
        open_source: "阅读来源",
        buy_heading: "购买或获取",
        buy_intro: "比较可用的供应商信息。购买前请向供应商确认规格、文件和运输条款。",
        no_offers_title: "暂无购买选项",
        no_offers_body: "我们尚未找到该矿物的当前供应商信息。",
        price_on_request: "价格需询价",
        price_unit: "件",
        price_lot: "批",
        price_package: "包装",
        minimum_order: "最小订购量",
        not_specified: "未注明",
        purity: "纯度",
        grade: "等级",
        origin: "产地",
        last_checked: "最近核查",
        open_provider: "查看供应商",
        stock_in: "有现货",
        stock_limited: "供应有限",
        stock_made_to_order: "按需生产",
        stock_quote: "请咨询供应情况",
        stock_out: "缺货",
        stock_unknown: "供应情况未确认",
    }
}

fn registry_ja() -> RegistryText {
    RegistryText {
        theme_toggle: "ダークモードを切り替える",
        eyebrow: "Waajacu 鉱物検索",
        title: "世界鉱物レジストリ",
        subtitle: "鉱物を検索し、科学的な出典を確認し、現在の入手方法を探せます。",
        source_links: "科学的資料",
        buying_options: "購入先",
        search_label: "名称、化学式、識別子、別名で検索",
        search_placeholder: "石英、SiO₂、紫水晶",
        search_action: "検索",
        empty_results: "一致する鉱物が見つかりません。別の名称、化学式、識別子をお試しください。",
        pagination_showing: "表示",
        pagination_of: "全",
        pagination_results: "件",
        pagination_page: "ページ",
        pagination_previous: "前へ",
        pagination_next: "次へ",
        kind_mineral: "鉱物",
        ima_number: "IMA番号",
        ima_symbol: "IMA記号",
        mineral_species: "鉱物種",
        nomenclature_ima_approved: "IMA承認",
        nomenclature_recognized: "承認済み（歴史的鉱物種）",
        nomenclature_redefined: "再定義済み",
        nomenclature_renamed: "改名済み",
        nomenclature_uncertain: "承認状況不明",
        nomenclature_questionable: "疑義のあるステータス",
        nomenclature_discredited: "認定取消",
        nomenclature_unknown: "ステータス未分類",
        valid_mineral_species: "有効な鉱物種",
        mineral_facts: "鉱物情報",
        nomenclature: "命名ステータス",
        source_status_approved: "IMAにより承認",
        source_status_grandfathered: "承認された既存の鉱物種",
        source_status_redefined: "IMAにより定義を改訂",
        source_status_renamed: "正式に改名",
        source_status_uncertain: "IMAの承認状況は不明",
        source_status_questionable: "ステータスに疑義あり",
        source_status_discredited: "有効な鉱物種としての認定を取消済み",
        source_status_unknown: "公式ステータス未分類",
        discovery_country: "発見国",
        official_identity_coverage_note: "この項目では現在、鉱物の公式な同一性と命名ステータスを記録しています。産状、結晶構造、物理的・光学的性質はまだ追加されていません。",
        published_references: "公表文献",
        source_and_license: "出典とライセンス",
        formula: "化学式",
        evidence: "科学的根拠",
        supports: "裏付ける内容",
        source_license: "出典ライセンス",
        attribution: "出典の帰属表示",
        attribution_party: "クレジット",
        source_work: "原著作物",
        license_terms: "ライセンス条件",
        changes_made: "変更内容",
        no_endorsement: "推奨を意味しません",
        derived_data_license: "派生データのライセンス",
        availability: "入手状況",
        detail_suffix: "鉱物記録",
        identity: "基本情報",
        family: "分類",
        identifiers: "識別子",
        properties: "特性",
        safety: "安全性",
        status: "知識の状態",
        status_note: "この状態は現在までの確認段階を示します。科学的資料と販売情報は別々に評価されます。",
        status_preliminary: "予備情報",
        status_sourced: "出典あり",
        status_reviewed: "確認済み",
        status_verified: "検証済み",
        status_disputed: "確認中",
        evidence_empty: "科学的資料はまだ登録されていません。この記録を調査の出発点とし、独自に確認してください。",
        source_retrieved: "参照日",
        open_source: "資料を読む",
        buy_heading: "購入・調達",
        buy_intro: "利用可能な販売情報を比較できます。購入前に仕様、書類、配送条件を供給元へ確認してください。",
        no_offers_title: "現在の購入先はありません",
        no_offers_body: "この鉱物について現在有効な販売情報はまだ見つかっていません。",
        price_on_request: "要見積もり",
        price_unit: "単位",
        price_lot: "ロット",
        price_package: "包装",
        minimum_order: "最低注文量",
        not_specified: "記載なし",
        purity: "純度",
        grade: "グレード",
        origin: "原産地",
        last_checked: "最終確認",
        open_provider: "供給元を見る",
        stock_in: "在庫あり",
        stock_limited: "在庫わずか",
        stock_made_to_order: "受注生産",
        stock_quote: "在庫はお問い合わせください",
        stock_out: "在庫なし",
        stock_unknown: "在庫未確認",
    }
}

fn registry_ar() -> RegistryText {
    RegistryText {
        theme_toggle: "تبديل الوضع الداكن",
        eyebrow: "اكتشاف المعادن من Waajacu",
        title: "السجل العالمي للمعادن",
        subtitle: "ابحث عن المعادن، وتتبّع مصادرها العلمية، واكتشف الطرق الحالية للحصول عليها.",
        source_links: "المصادر العلمية",
        buying_options: "خيارات الشراء",
        search_label: "البحث بالاسم أو الصيغة أو المعرّف أو المرادف",
        search_placeholder: "كوارتز، SiO₂، جمشت",
        search_action: "بحث",
        empty_results: "لم نعثر على معادن مطابقة. جرّب اسماً أو صيغة أو معرّفاً آخر.",
        pagination_showing: "عرض",
        pagination_of: "من",
        pagination_results: "نتيجة",
        pagination_page: "الصفحة",
        pagination_previous: "السابق",
        pagination_next: "التالي",
        kind_mineral: "معدن",
        ima_number: "رقم IMA",
        ima_symbol: "رمز IMA",
        mineral_species: "نوع معدني",
        nomenclature_ima_approved: "معتمد من IMA",
        nomenclature_recognized: "معترف به (تاريخي)",
        nomenclature_redefined: "أعيد تعريفه",
        nomenclature_renamed: "أعيدت تسميته",
        nomenclature_uncertain: "الموافقة غير مؤكدة",
        nomenclature_questionable: "حالة مشكوك فيها",
        nomenclature_discredited: "سُحب الاعتراف به",
        nomenclature_unknown: "الحالة غير مصنفة",
        valid_mineral_species: "نوع معدني صالح",
        mineral_facts: "حقائق المعدن",
        nomenclature: "التسمية المعدنية",
        source_status_approved: "معتمد من IMA",
        source_status_grandfathered: "نوع راسخ ومعترف به",
        source_status_redefined: "راجعت IMA تعريفه",
        source_status_renamed: "أعيدت تسميته رسمياً",
        source_status_uncertain: "موافقة IMA غير مؤكدة",
        source_status_questionable: "تُعد حالته موضع شك",
        source_status_discredited: "لم يعد معترفاً به كنوع صالح",
        source_status_unknown: "الحالة الرسمية غير مصنفة",
        discovery_country: "بلد الاكتشاف",
        official_identity_coverage_note: "توثق هذه الصفحة حالياً الهوية الرسمية للمعدن وتسميته. ولم تُضف بعد معلومات وجوده وبنيته البلورية وخصائصه الفيزيائية والبصرية.",
        published_references: "مراجع منشورة",
        source_and_license: "المصدر والترخيص",
        formula: "الصيغة",
        evidence: "الأدلة العلمية",
        supports: "يدعم",
        source_license: "ترخيص المصدر",
        attribution: "نَسب المصدر",
        attribution_party: "الإسناد",
        source_work: "العمل المصدر",
        license_terms: "شروط الترخيص",
        changes_made: "التغييرات المنفذة",
        no_endorsement: "لا يعني التأييد",
        derived_data_license: "ترخيص البيانات المشتقة",
        availability: "التوفر",
        detail_suffix: "سجل المعدن",
        identity: "التعريف",
        family: "العائلة",
        identifiers: "المعرّفات",
        properties: "الخصائص",
        safety: "السلامة",
        status: "حالة المعرفة",
        status_note: "تعكس الحالة مقدار المراجعة المنجزة حتى الآن. تُقيّم المصادر العلمية وعروض البائعين بصورة منفصلة.",
        status_preliminary: "أولي",
        status_sourced: "مرفق بمصادر",
        status_reviewed: "تمت مراجعته",
        status_verified: "موثّق",
        status_disputed: "قيد المراجعة",
        evidence_empty: "لم يُرفق مصدر علمي بعد. استخدم هذا السجل كنقطة بداية وتحقق منه بصورة مستقلة.",
        source_retrieved: "تاريخ الاطلاع",
        open_source: "قراءة المصدر",
        buy_heading: "الشراء أو التوريد",
        buy_intro: "قارن عروض الموردين المتاحة. تأكد من المواصفات والوثائق وشروط الشحن مع المورد قبل الشراء.",
        no_offers_title: "لا توجد خيارات شراء حالية",
        no_offers_body: "لم نعثر بعد على عرض مورّد حالي لهذا المعدن.",
        price_on_request: "السعر عند الطلب",
        price_unit: "وحدة",
        price_lot: "دفعة",
        price_package: "عبوة",
        minimum_order: "الحد الأدنى للطلب",
        not_specified: "غير محدد",
        purity: "النقاوة",
        grade: "الدرجة",
        origin: "المنشأ",
        last_checked: "آخر تحقق",
        open_provider: "عرض المورد",
        stock_in: "متوفر",
        stock_limited: "توفر محدود",
        stock_made_to_order: "يُصنع حسب الطلب",
        stock_quote: "تواصل لمعرفة التوفر",
        stock_out: "غير متوفر",
        stock_unknown: "التوفر غير مؤكد",
    }
}

fn registry_hi() -> RegistryText {
    RegistryText {
        theme_toggle: "डार्क मोड बदलें",
        eyebrow: "Waajacu खनिज खोज",
        title: "वैश्विक खनिज रजिस्ट्री",
        subtitle: "खनिज खोजें, उनके वैज्ञानिक स्रोत देखें और उन्हें प्राप्त करने के मौजूदा विकल्प जानें।",
        source_links: "वैज्ञानिक स्रोत",
        buying_options: "खरीद विकल्प",
        search_label: "नाम, सूत्र, पहचान संख्या या पर्याय से खोजें",
        search_placeholder: "क्वार्ट्ज़, SiO₂, एमेथिस्ट",
        search_action: "खोजें",
        empty_results: "कोई मिलता-जुलता खनिज नहीं मिला। दूसरा नाम, सूत्र या पहचान संख्या आज़माएँ।",
        pagination_showing: "दिखाए जा रहे हैं",
        pagination_of: "में से",
        pagination_results: "परिणाम",
        pagination_page: "पृष्ठ",
        pagination_previous: "पिछला",
        pagination_next: "अगला",
        kind_mineral: "खनिज",
        ima_number: "IMA संख्या",
        ima_symbol: "IMA प्रतीक",
        mineral_species: "खनिज प्रजाति",
        nomenclature_ima_approved: "IMA द्वारा स्वीकृत",
        nomenclature_recognized: "मान्य (ऐतिहासिक)",
        nomenclature_redefined: "पुनर्परिभाषित",
        nomenclature_renamed: "पुनर्नामित",
        nomenclature_uncertain: "स्वीकृति अनिश्चित",
        nomenclature_questionable: "स्थिति संदिग्ध",
        nomenclature_discredited: "अमान्य घोषित",
        nomenclature_unknown: "स्थिति वर्गीकृत नहीं",
        valid_mineral_species: "मान्य खनिज प्रजाति",
        mineral_facts: "खनिज तथ्य",
        nomenclature: "नामकरण",
        source_status_approved: "IMA द्वारा स्वीकृत",
        source_status_grandfathered: "स्थापित और स्वीकृत प्रजाति",
        source_status_redefined: "IMA द्वारा परिभाषा संशोधित",
        source_status_renamed: "आधिकारिक रूप से पुनर्नामित",
        source_status_uncertain: "IMA की स्वीकृति अनिश्चित है",
        source_status_questionable: "स्थिति को संदिग्ध माना जाता है",
        source_status_discredited: "अब मान्य प्रजाति के रूप में स्वीकार नहीं",
        source_status_unknown: "आधिकारिक स्थिति वर्गीकृत नहीं",
        discovery_country: "खोज का देश",
        official_identity_coverage_note: "यह प्रविष्टि अभी खनिज की आधिकारिक पहचान और नामकरण का दस्तावेज़ प्रस्तुत करती है। इसकी उपस्थिति, क्रिस्टल संरचना तथा भौतिक और प्रकाशीय गुण अभी जोड़े नहीं गए हैं।",
        published_references: "प्रकाशित संदर्भ",
        source_and_license: "स्रोत और लाइसेंस",
        formula: "सूत्र",
        evidence: "वैज्ञानिक प्रमाण",
        supports: "समर्थन करता है",
        source_license: "स्रोत लाइसेंस",
        attribution: "स्रोत श्रेय",
        attribution_party: "श्रेय",
        source_work: "मूल कृति",
        license_terms: "लाइसेंस की शर्तें",
        changes_made: "किए गए परिवर्तन",
        no_endorsement: "समर्थन अभिप्रेत नहीं",
        derived_data_license: "व्युत्पन्न डेटा लाइसेंस",
        availability: "उपलब्धता",
        detail_suffix: "खनिज रिकॉर्ड",
        identity: "पहचान",
        family: "परिवार",
        identifiers: "पहचान संख्याएँ",
        properties: "गुण",
        safety: "सुरक्षा",
        status: "ज्ञान की स्थिति",
        status_note: "स्थिति अब तक पूरी हुई समीक्षा को दर्शाती है। वैज्ञानिक स्रोतों और विक्रेता सूचियों का अलग-अलग मूल्यांकन किया जाता है।",
        status_preliminary: "प्रारंभिक",
        status_sourced: "स्रोत उपलब्ध",
        status_reviewed: "समीक्षित",
        status_verified: "सत्यापित",
        status_disputed: "समीक्षा जारी",
        evidence_empty: "अभी कोई वैज्ञानिक स्रोत जुड़ा नहीं है। इस रिकॉर्ड को शोध की शुरुआत मानें और स्वतंत्र रूप से सत्यापित करें।",
        source_retrieved: "देखने की तारीख",
        open_source: "स्रोत पढ़ें",
        buy_heading: "खरीदें या प्राप्त करें",
        buy_intro: "उपलब्ध आपूर्तिकर्ता सूचियों की तुलना करें। खरीदने से पहले आपूर्तिकर्ता से विनिर्देश, दस्तावेज़ और शिपिंग शर्तें पक्की करें।",
        no_offers_title: "अभी कोई खरीद विकल्प नहीं",
        no_offers_body: "इस खनिज के लिए अभी कोई मौजूदा आपूर्तिकर्ता सूची नहीं मिली है।",
        price_on_request: "माँगने पर मूल्य",
        price_unit: "इकाई",
        price_lot: "खेप",
        price_package: "पैकेज",
        minimum_order: "न्यूनतम ऑर्डर",
        not_specified: "उल्लेख नहीं",
        purity: "शुद्धता",
        grade: "ग्रेड",
        origin: "मूल स्थान",
        last_checked: "अंतिम जाँच",
        open_provider: "आपूर्तिकर्ता देखें",
        stock_in: "स्टॉक में",
        stock_limited: "सीमित उपलब्धता",
        stock_made_to_order: "ऑर्डर पर तैयार",
        stock_quote: "उपलब्धता के लिए संपर्क करें",
        stock_out: "स्टॉक में नहीं",
        stock_unknown: "उपलब्धता की पुष्टि नहीं",
    }
}

fn review_en() -> ReviewText {
    ReviewText {
        queue_link: "Review queue",
        title: "Pending mineral reviews",
        subtitle: "Inspect each imported mineral revision and its evidence before deciding whether it should be published.",
        back_to_admin: "Back to admin",
        empty: "No mineral revisions are waiting for review.",
        pending_candidates: "pending reviews",
        review_id: "Review ID",
        revision: "Revision",
        creates_new: "Creates new profile",
        updates_existing: "Updates existing profile",
        view_current: "View current profile",
        submitted_source: "Submitted by",
        submitted_at: "Submitted",
        quality: "Data quality",
        slug: "Record slug",
        cas_number: "CAS number",
        synonyms: "Synonyms",
        record_license: "Record license",
        publisher: "Publisher",
        claim_scope: "Supports",
        claim_value: "Claimed value",
        claim_locator: "Source locator",
        claim_note: "Claim note",
        claim_details: "Complete claim",
        confidence: "Claim confidence",
        evidence_state: "Evidence status",
        retrieved_at: "Retrieved",
        content_hash: "Content hash",
        complete_payload: "Complete staged payload",
        complete_payload_hint: "This is the exact submitted revision used for this decision.",
        operator_note: "Operator note",
        operator_note_placeholder: "Record what you checked and why you made this decision.",
        note_required: "A note is required for every decision.",
        approve: "Approve and publish",
        reject: "Reject",
        approved_notice: "Mineral revision approved and published.",
        rejected_notice: "Mineral revision rejected.",
        decision_warning: "Approval publishes this exact revision but does not upgrade its scientific verification status. Rejection keeps any currently published revision unchanged.",
    }
}

fn review_es() -> ReviewText {
    ReviewText {
        queue_link: "Cola de revisión",
        title: "Revisiones de minerales pendientes",
        subtitle: "Examina cada revisión importada y sus fuentes antes de decidir si debe publicarse.",
        back_to_admin: "Volver a administración",
        empty: "No hay revisiones de minerales pendientes.",
        pending_candidates: "revisiones pendientes",
        review_id: "ID de revisión",
        revision: "Revisión",
        creates_new: "Crea un perfil nuevo",
        updates_existing: "Actualiza un perfil existente",
        view_current: "Ver perfil actual",
        submitted_source: "Enviado por",
        submitted_at: "Enviado",
        quality: "Calidad de los datos",
        slug: "Identificador del registro",
        cas_number: "Número CAS",
        synonyms: "Sinónimos",
        record_license: "Licencia del registro",
        publisher: "Editor",
        claim_scope: "Respalda",
        claim_value: "Valor afirmado",
        claim_locator: "Ubicación en la fuente",
        claim_note: "Nota de la afirmación",
        claim_details: "Afirmación completa",
        confidence: "Confianza de la afirmación",
        evidence_state: "Estado de la fuente",
        retrieved_at: "Consultado",
        content_hash: "Huella del contenido",
        complete_payload: "Contenido completo en revisión",
        complete_payload_hint: "Esta es la revisión exacta enviada para esta decisión.",
        operator_note: "Nota del operador",
        operator_note_placeholder: "Indica qué comprobaste y por qué tomaste esta decisión.",
        note_required: "Cada decisión requiere una nota.",
        approve: "Aprobar y publicar",
        reject: "Rechazar",
        approved_notice: "La revisión del mineral fue aprobada y publicada.",
        rejected_notice: "La revisión del mineral fue rechazada.",
        decision_warning: "La aprobación publica esta revisión exacta, pero no eleva su estado de verificación científica. El rechazo no cambia ninguna revisión ya publicada.",
    }
}

fn review_cs() -> ReviewText {
    ReviewText {
        queue_link: "Fronta ke kontrole",
        title: "Čekající kontroly minerálů",
        subtitle: "Před rozhodnutím o zveřejnění zkontrolujte každou importovanou revizi a její zdroje.",
        back_to_admin: "Zpět do administrace",
        empty: "Na kontrolu nečekají žádné revize minerálů.",
        pending_candidates: "čekajících kontrol",
        review_id: "ID kontroly",
        revision: "Revize",
        creates_new: "Vytvoří nový profil",
        updates_existing: "Aktualizuje existující profil",
        view_current: "Zobrazit současný profil",
        submitted_source: "Odeslal",
        submitted_at: "Odesláno",
        quality: "Kvalita dat",
        slug: "Identifikátor záznamu",
        cas_number: "Číslo CAS",
        synonyms: "Synonyma",
        record_license: "Licence záznamu",
        publisher: "Vydavatel",
        claim_scope: "Dokládá",
        claim_value: "Tvrzená hodnota",
        claim_locator: "Umístění ve zdroji",
        claim_note: "Poznámka k tvrzení",
        claim_details: "Úplné tvrzení",
        confidence: "Spolehlivost tvrzení",
        evidence_state: "Stav podkladu",
        retrieved_at: "Získáno",
        content_hash: "Otisk obsahu",
        complete_payload: "Úplná kontrolovaná data",
        complete_payload_hint: "Toto je přesně ta odeslaná revize, o které se rozhoduje.",
        operator_note: "Poznámka operátora",
        operator_note_placeholder: "Uveďte, co jste ověřili a proč jste takto rozhodli.",
        note_required: "Každé rozhodnutí vyžaduje poznámku.",
        approve: "Schválit a zveřejnit",
        reject: "Zamítnout",
        approved_notice: "Revize minerálu byla schválena a zveřejněna.",
        rejected_notice: "Revize minerálu byla zamítnuta.",
        decision_warning: "Schválení zveřejní přesně tuto revizi, ale nezvyšuje stav jejího vědeckého ověření. Zamítnutí nezmění žádnou již zveřejněnou revizi.",
    }
}

fn review_de() -> ReviewText {
    ReviewText {
        queue_link: "Prüfwarteschlange",
        title: "Ausstehende Mineralprüfungen",
        subtitle: "Prüfen Sie jede importierte Revision und ihre Quellen, bevor Sie über die Veröffentlichung entscheiden.",
        back_to_admin: "Zurück zur Verwaltung",
        empty: "Es warten keine Mineralrevisionen auf Prüfung.",
        pending_candidates: "ausstehende Prüfungen",
        review_id: "Prüf-ID",
        revision: "Revision",
        creates_new: "Erstellt ein neues Profil",
        updates_existing: "Aktualisiert ein vorhandenes Profil",
        view_current: "Aktuelles Profil ansehen",
        submitted_source: "Eingereicht von",
        submitted_at: "Eingereicht",
        quality: "Datenqualität",
        slug: "Datensatzkennung",
        cas_number: "CAS-Nummer",
        synonyms: "Synonyme",
        record_license: "Datensatzlizenz",
        publisher: "Herausgeber",
        claim_scope: "Belegt",
        claim_value: "Behaupteter Wert",
        claim_locator: "Fundstelle in der Quelle",
        claim_note: "Hinweis zur Aussage",
        claim_details: "Vollständige Aussage",
        confidence: "Verlässlichkeit der Aussage",
        evidence_state: "Quellenstatus",
        retrieved_at: "Abgerufen",
        content_hash: "Inhaltsprüfsumme",
        complete_payload: "Vollständiger Prüfdatensatz",
        complete_payload_hint: "Dies ist exakt die eingereichte Revision, über die entschieden wird.",
        operator_note: "Prüfnotiz",
        operator_note_placeholder: "Halten Sie fest, was geprüft wurde und warum Sie so entschieden haben.",
        note_required: "Für jede Entscheidung ist eine Notiz erforderlich.",
        approve: "Freigeben und veröffentlichen",
        reject: "Ablehnen",
        approved_notice: "Die Mineralrevision wurde freigegeben und veröffentlicht.",
        rejected_notice: "Die Mineralrevision wurde abgelehnt.",
        decision_warning: "Die Freigabe veröffentlicht genau diese Revision, ändert aber ihren wissenschaftlichen Prüfstatus nicht. Eine Ablehnung ändert keine bereits veröffentlichte Revision.",
    }
}

fn review_fr() -> ReviewText {
    ReviewText {
        queue_link: "File de révision",
        title: "Révisions de minéraux en attente",
        subtitle: "Examinez chaque révision importée et ses sources avant de décider de sa publication.",
        back_to_admin: "Retour à l’administration",
        empty: "Aucune révision de minéral n’attend de validation.",
        pending_candidates: "révisions en attente",
        review_id: "ID de révision",
        revision: "Révision",
        creates_new: "Crée une nouvelle fiche",
        updates_existing: "Met à jour une fiche existante",
        view_current: "Voir la fiche actuelle",
        submitted_source: "Soumis par",
        submitted_at: "Soumis le",
        quality: "Qualité des données",
        slug: "Identifiant du registre",
        cas_number: "Numéro CAS",
        synonyms: "Synonymes",
        record_license: "Licence du registre",
        publisher: "Éditeur",
        claim_scope: "Étaye",
        claim_value: "Valeur affirmée",
        claim_locator: "Emplacement dans la source",
        claim_note: "Note sur l’affirmation",
        claim_details: "Affirmation complète",
        confidence: "Confiance dans l’affirmation",
        evidence_state: "État de la source",
        retrieved_at: "Consulté le",
        content_hash: "Empreinte du contenu",
        complete_payload: "Contenu complet à examiner",
        complete_payload_hint: "Il s’agit exactement de la révision soumise pour cette décision.",
        operator_note: "Note de l’opérateur",
        operator_note_placeholder: "Indiquez ce que vous avez vérifié et la raison de votre décision.",
        note_required: "Une note est requise pour chaque décision.",
        approve: "Approuver et publier",
        reject: "Rejeter",
        approved_notice: "La révision du minéral a été approuvée et publiée.",
        rejected_notice: "La révision du minéral a été rejetée.",
        decision_warning: "L’approbation publie précisément cette révision, sans relever son niveau de vérification scientifique. Le rejet ne modifie aucune révision déjà publiée.",
    }
}

fn review_pt() -> ReviewText {
    ReviewText {
        queue_link: "Fila de revisão",
        title: "Revisões de minerais pendentes",
        subtitle: "Analise cada revisão importada e suas fontes antes de decidir se ela deve ser publicada.",
        back_to_admin: "Voltar à administração",
        empty: "Não há revisões de minerais aguardando análise.",
        pending_candidates: "revisões pendentes",
        review_id: "ID da revisão",
        revision: "Revisão",
        creates_new: "Cria um novo perfil",
        updates_existing: "Atualiza um perfil existente",
        view_current: "Ver perfil atual",
        submitted_source: "Enviado por",
        submitted_at: "Enviado em",
        quality: "Qualidade dos dados",
        slug: "Identificador do registro",
        cas_number: "Número CAS",
        synonyms: "Sinônimos",
        record_license: "Licença do registro",
        publisher: "Publicador",
        claim_scope: "Sustenta",
        claim_value: "Valor declarado",
        claim_locator: "Localização na fonte",
        claim_note: "Nota da afirmação",
        claim_details: "Afirmação completa",
        confidence: "Confiança na afirmação",
        evidence_state: "Estado da fonte",
        retrieved_at: "Consultado em",
        content_hash: "Identificador do conteúdo",
        complete_payload: "Conteúdo completo em revisão",
        complete_payload_hint: "Esta é exatamente a revisão enviada para esta decisão.",
        operator_note: "Nota do operador",
        operator_note_placeholder: "Registre o que foi verificado e o motivo da decisão.",
        note_required: "Toda decisão exige uma nota.",
        approve: "Aprovar e publicar",
        reject: "Rejeitar",
        approved_notice: "A revisão do mineral foi aprovada e publicada.",
        rejected_notice: "A revisão do mineral foi rejeitada.",
        decision_warning: "A aprovação publica exatamente esta revisão, mas não eleva seu status de verificação científica. A rejeição não altera nenhuma revisão já publicada.",
    }
}

fn review_zh() -> ReviewText {
    ReviewText {
        queue_link: "审核队列",
        title: "待审核的矿物记录",
        subtitle: "在决定是否发布之前，请检查每个导入版本及其科学来源。",
        back_to_admin: "返回管理页",
        empty: "目前没有等待审核的矿物版本。",
        pending_candidates: "项待审核记录",
        review_id: "审核编号",
        revision: "版本",
        creates_new: "创建新档案",
        updates_existing: "更新现有档案",
        view_current: "查看当前档案",
        submitted_source: "提交来源",
        submitted_at: "提交时间",
        quality: "数据质量",
        slug: "记录标识",
        cas_number: "CAS 编号",
        synonyms: "同义名",
        record_license: "记录许可",
        publisher: "发布机构",
        claim_scope: "支持内容",
        claim_value: "声明值",
        claim_locator: "来源位置",
        claim_note: "声明备注",
        claim_details: "完整声明",
        confidence: "声明可信度",
        evidence_state: "来源状态",
        retrieved_at: "检索时间",
        content_hash: "内容哈希",
        complete_payload: "完整待审内容",
        complete_payload_hint: "这是本次决定所依据的确切提交版本。",
        operator_note: "审核备注",
        operator_note_placeholder: "记录已核查的内容以及作出此决定的原因。",
        note_required: "每项决定都必须填写备注。",
        approve: "批准并发布",
        reject: "拒绝",
        approved_notice: "矿物版本已批准并发布。",
        rejected_notice: "矿物版本已拒绝。",
        decision_warning:
            "批准将发布此确切版本，但不会提升其科学验证状态。拒绝不会改变任何已经发布的版本。",
    }
}

fn review_ja() -> ReviewText {
    ReviewText {
        queue_link: "レビュー待ち",
        title: "審査待ちの鉱物レコード",
        subtitle: "公開を決定する前に、インポートされた各版と科学的根拠を確認してください。",
        back_to_admin: "管理画面に戻る",
        empty: "審査待ちの鉱物版はありません。",
        pending_candidates: "件の審査待ち",
        review_id: "レビューID",
        revision: "版",
        creates_new: "新しいプロフィールを作成",
        updates_existing: "既存プロフィールを更新",
        view_current: "現在のプロフィールを見る",
        submitted_source: "提出元",
        submitted_at: "提出日時",
        quality: "データ品質",
        slug: "レコード識別子",
        cas_number: "CAS番号",
        synonyms: "同義語",
        record_license: "レコードライセンス",
        publisher: "発行元",
        claim_scope: "裏付ける内容",
        claim_value: "主張される値",
        claim_locator: "出典内の位置",
        claim_note: "主張メモ",
        claim_details: "主張の全内容",
        confidence: "主張の信頼度",
        evidence_state: "根拠の状態",
        retrieved_at: "取得日時",
        content_hash: "コンテンツハッシュ",
        complete_payload: "審査対象の全内容",
        complete_payload_hint: "これは、この判断に用いる提出済みの版そのものです。",
        operator_note: "審査メモ",
        operator_note_placeholder: "確認した内容と、この判断に至った理由を記録してください。",
        note_required: "すべての判断にメモが必要です。",
        approve: "承認して公開",
        reject: "却下",
        approved_notice: "鉱物の版を承認して公開しました。",
        rejected_notice: "鉱物の版を却下しました。",
        decision_warning: "承認すると、この版がそのまま公開されますが、科学的検証ステータスは上がりません。却下しても、すでに公開中の版は変更されません。",
    }
}

fn review_ar() -> ReviewText {
    ReviewText {
        queue_link: "قائمة انتظار المراجعة",
        title: "مراجعات المعادن المعلّقة",
        subtitle: "افحص كل نسخة مستوردة ومصادرها العلمية قبل اتخاذ قرار النشر.",
        back_to_admin: "العودة إلى الإدارة",
        empty: "لا توجد نسخ معادن بانتظار المراجعة.",
        pending_candidates: "مراجعات معلّقة",
        review_id: "معرّف المراجعة",
        revision: "النسخة",
        creates_new: "ينشئ ملفًا جديدًا",
        updates_existing: "يحدّث ملفًا موجودًا",
        view_current: "عرض الملف الحالي",
        submitted_source: "مصدر الإرسال",
        submitted_at: "وقت الإرسال",
        quality: "جودة البيانات",
        slug: "معرّف السجل",
        cas_number: "رقم CAS",
        synonyms: "المرادفات",
        record_license: "ترخيص السجل",
        publisher: "الناشر",
        claim_scope: "يدعم",
        claim_value: "القيمة المدعاة",
        claim_locator: "الموضع في المصدر",
        claim_note: "ملاحظة الادعاء",
        claim_details: "الادعاء الكامل",
        confidence: "موثوقية الادعاء",
        evidence_state: "حالة المصدر",
        retrieved_at: "تاريخ الاسترجاع",
        content_hash: "بصمة المحتوى",
        complete_payload: "المحتوى الكامل قيد المراجعة",
        complete_payload_hint: "هذه هي النسخة المقدمة نفسها التي يستند إليها هذا القرار.",
        operator_note: "ملاحظة المراجع",
        operator_note_placeholder: "سجّل ما تحققت منه وسبب اتخاذ هذا القرار.",
        note_required: "تُطلب ملاحظة مع كل قرار.",
        approve: "الموافقة والنشر",
        reject: "رفض",
        approved_notice: "تمت الموافقة على نسخة المعدن ونشرها.",
        rejected_notice: "تم رفض نسخة المعدن.",
        decision_warning: "تنشر الموافقة هذه النسخة تحديدًا، لكنها لا ترفع حالة التحقق العلمي. ولا يغيّر الرفض أي نسخة منشورة حاليًا.",
    }
}

fn review_hi() -> ReviewText {
    ReviewText {
        queue_link: "समीक्षा कतार",
        title: "लंबित खनिज समीक्षाएँ",
        subtitle: "प्रकाशन का निर्णय लेने से पहले हर आयातित संस्करण और उसके वैज्ञानिक स्रोतों की जाँच करें।",
        back_to_admin: "प्रशासन पर वापस जाएँ",
        empty: "समीक्षा के लिए कोई खनिज संस्करण लंबित नहीं है।",
        pending_candidates: "लंबित समीक्षाएँ",
        review_id: "समीक्षा आईडी",
        revision: "संस्करण",
        creates_new: "नई प्रोफ़ाइल बनाता है",
        updates_existing: "मौजूदा प्रोफ़ाइल अपडेट करता है",
        view_current: "वर्तमान प्रोफ़ाइल देखें",
        submitted_source: "प्रस्तुतकर्ता",
        submitted_at: "प्रस्तुत समय",
        quality: "डेटा गुणवत्ता",
        slug: "रिकॉर्ड पहचान",
        cas_number: "CAS संख्या",
        synonyms: "पर्याय",
        record_license: "रिकॉर्ड लाइसेंस",
        publisher: "प्रकाशक",
        claim_scope: "समर्थन करता है",
        claim_value: "दावा किया गया मान",
        claim_locator: "स्रोत में स्थान",
        claim_note: "दावे की टिप्पणी",
        claim_details: "पूरा दावा",
        confidence: "दावे का विश्वास स्तर",
        evidence_state: "स्रोत की स्थिति",
        retrieved_at: "प्राप्ति समय",
        content_hash: "सामग्री हैश",
        complete_payload: "समीक्षा के लिए पूरा डेटा",
        complete_payload_hint: "यह उसी प्रस्तुत संस्करण की पूरी सामग्री है जिस पर निर्णय लिया जा रहा है।",
        operator_note: "समीक्षक टिप्पणी",
        operator_note_placeholder: "लिखें कि आपने क्या जाँचा और यह निर्णय क्यों लिया।",
        note_required: "हर निर्णय के लिए टिप्पणी आवश्यक है।",
        approve: "स्वीकृत कर प्रकाशित करें",
        reject: "अस्वीकार करें",
        approved_notice: "खनिज संस्करण स्वीकृत और प्रकाशित किया गया।",
        rejected_notice: "खनिज संस्करण अस्वीकार किया गया।",
        decision_warning:
            "स्वीकृति इसी संस्करण को प्रकाशित करती है, लेकिन उसकी वैज्ञानिक सत्यापन स्थिति को नहीं बढ़ाती। अस्वीकृति पहले से प्रकाशित किसी संस्करण को नहीं बदलती।",
    }
}

fn ingestion_text(language: Language) -> IngestionText {
    let mut text = IngestionText {
        link: "Mineral database",
        title: "Mineral database",
        subtitle: "Official mineral releases and their publication status.",
        back_to_admin: "Back to admin",
        individual_review_link: "Review minerals individually",
        create_title: "Create batch",
        create_hint: "Submit the complete source and release manifest before uploading numbered chunks.",
        manifest_payload: "Manifest JSON",
        dataset: "Dataset",
        source_name: "Source",
        source_url: "Source URL",
        attribution_review: "Source",
        historical_attribution_missing:
            "Source credit is incomplete. This release cannot be published.",
        release_version: "Release version",
        released_at: "Released at",
        retrieved_at: "Retrieved at",
        manifest_hash: "Manifest hash",
        artifact_hash: "Artifact hash",
        parser_name: "Parser",
        parser_version: "Parser version",
        parser_revision: "Parser revision",
        parser_configuration_hash: "Parser configuration hash",
        record_license: "Record license",
        expected_chunks: "Expected chunks",
        expected_records: "Expected records",
        create_batch: "Create batch",
        upload_title: "Upload chunk",
        upload_hint: "Upload each numbered JSON chunk to its batch. Identical retries are safe.",
        batch_id: "Batch ID",
        chunk_number: "Chunk number",
        chunk_payload: "Chunk JSON",
        upload_chunk: "Upload chunk",
        finalize_title: "Validate batch",
        finalize_hint: "Check completeness, analyze every mineral record, and produce the validation report.",
        finalize_validate: "Finalize and validate",
        batches_title: "Mineral releases",
        empty: "No mineral releases yet.",
        status: "Status",
        status_receiving: "Receiving",
        status_ready: "Ready to publish",
        status_needs_attention: "Needs attention",
        status_approved: "Approved",
        status_rejected: "Rejected",
        progress: "Progress",
        created_at: "Created at",
        finalized_at: "Finalized at",
        uploaded_chunks: "Uploaded chunks",
        records: "Records",
        created: "New",
        adopted: "Matched",
        updated: "Changed",
        unchanged: "Unchanged",
        missing: "Missing",
        blockers: "Blockers",
        identity_warnings: "Warnings",
        anomaly_samples: "Problems to review",
        no_anomalies: "No problems found.",
        review_samples: "Review samples",
        source_record: "Source record",
        nomenclature_status: "Nomenclature status",
        valid_species: "Valid species",
        yes: "Yes",
        no: "No",
        report_hash: "Report hash",
        base_batch: "Base batch",
        decision_title: "Publish this list?",
        decision_warning: "Publishing makes these records visible in the mineral catalog. Rejecting keeps them private.",
        acknowledge_warning: "I checked the source, record totals, and any warnings shown above.",
        confirmation: "Confirmation",
        confirmation_hint: "Type the release version exactly to confirm.",
        operator_note: "Operator note",
        operator_note_placeholder: "Add a short reason for this decision.",
        approve_release: "Publish minerals",
        reject_release: "Discard import",
        working: "Working…",
        request_failed: "The request failed.",
        batch_created: "Batch created.",
        chunk_uploaded: "Chunk uploaded.",
        validation_complete: "Validation complete.",
        approved_notice: "Minerals published.",
        rejected_notice: "Import discarded; no mineral records were published.",
    };

    match language {
        Language::En => {}
        Language::Es => {
            text.link = "Base de datos de minerales";
            text.title = "Base de datos de minerales";
            text.subtitle = "Publicaciones oficiales de minerales y su estado de publicación.";
            text.attribution_review = "Fuente";
            text.historical_attribution_missing =
                "Falta información sobre la fuente. Esta publicación no se puede publicar.";
            text.back_to_admin = "Volver a administración";
            text.individual_review_link = "Revisar minerales individualmente";
            text.create_title = "Crear lote";
            text.create_hint = "Envía el manifiesto completo de la fuente y la versión antes de cargar los fragmentos numerados.";
            text.manifest_payload = "Manifiesto JSON";
            text.dataset = "Conjunto de datos";
            text.source_name = "Nombre de la fuente";
            text.source_url = "URL de la fuente";
            text.release_version = "Versión de la entrega";
            text.released_at = "Publicado el";
            text.retrieved_at = "Consultado el";
            text.manifest_hash = "Hash del manifiesto";
            text.artifact_hash = "Hash del artefacto";
            text.parser_name = "Nombre del parser";
            text.parser_version = "Versión del parser";
            text.parser_revision = "Revisión del parser";
            text.parser_configuration_hash = "Hash de configuración del parser";
            text.record_license = "Licencia de los registros";
            text.expected_chunks = "Fragmentos previstos";
            text.expected_records = "Registros previstos";
            text.create_batch = "Crear lote";
            text.upload_title = "Cargar fragmento";
            text.upload_hint = "Carga en el lote cada fragmento JSON numerado. Los reintentos idénticos son seguros.";
            text.batch_id = "ID del lote";
            text.chunk_number = "Número de fragmento";
            text.chunk_payload = "Fragmento JSON";
            text.upload_chunk = "Cargar fragmento";
            text.finalize_title = "Validar lote";
            text.finalize_hint = "Comprueba que esté completo, analiza cada registro mineral y genera el informe de validación.";
            text.finalize_validate = "Finalizar y validar";
            text.batches_title = "Publicaciones de minerales";
            text.empty = "Aún no hay publicaciones de minerales.";
            text.status = "Estado";
            text.status_receiving = "Recibiendo";
            text.status_ready = "Listo";
            text.status_needs_attention = "Requiere atención";
            text.status_approved = "Aprobado";
            text.status_rejected = "Rechazado";
            text.progress = "Progreso";
            text.created_at = "Creado el";
            text.finalized_at = "Finalizado el";
            text.uploaded_chunks = "Fragmentos cargados";
            text.records = "Registros";
            text.created = "Creados";
            text.adopted = "Adoptados";
            text.updated = "Actualizados";
            text.unchanged = "Sin cambios";
            text.missing = "Ausentes";
            text.blockers = "Bloqueos";
            text.identity_warnings = "Advertencias de identidad";
            text.anomaly_samples = "Muestras de anomalías";
            text.no_anomalies = "No hay muestras de anomalías.";
            text.review_samples = "Muestras para revisión";
            text.source_record = "Registro de origen";
            text.nomenclature_status = "Estado nomenclatural";
            text.valid_species = "Especie válida";
            text.yes = "Sí";
            text.no = "No";
            text.report_hash = "Hash del informe";
            text.base_batch = "Lote base";
            text.decision_title = "Decisión de publicación";
            text.decision_warning = "Aprobar publica esta versión de datos minerales. Rechazarla la mantiene no disponible. Revisa el informe antes de continuar.";
            text.acknowledge_warning =
                "He revisado el informe de validación y comprendo esta decisión.";
            text.confirmation = "Confirmación";
            text.confirmation_hint = "Escribe exactamente la versión de la entrega para confirmar.";
            text.operator_note = "Nota del operador";
            text.operator_note_placeholder =
                "Registra las pruebas y el razonamiento de esta decisión.";
            text.approve_release = "Aprobar publicación";
            text.reject_release = "Rechazar publicación";
            text.working = "Procesando…";
            text.request_failed = "La solicitud falló.";
            text.batch_created = "Lote creado.";
            text.chunk_uploaded = "Fragmento cargado.";
            text.validation_complete = "Validación completada.";
            text.approved_notice = "Versión aprobada y publicada.";
            text.rejected_notice = "Versión rechazada; no se publicó ningún registro mineral.";
        }
        Language::Cs => {
            text.link = "Databáze minerálů";
            text.title = "Databáze minerálů";
            text.subtitle = "Oficiální vydání dat o minerálech a stav jejich zveřejnění.";
            text.attribution_review = "Zdroj";
            text.historical_attribution_missing =
                "Údaje o zdroji nejsou úplné. Toto vydání nelze zveřejnit.";
            text.back_to_admin = "Zpět do administrace";
            text.individual_review_link = "Kontrola jednotlivých minerálů";
            text.create_title = "Vytvořit dávku";
            text.create_hint =
                "Před nahráním očíslovaných částí odešlete úplný manifest zdroje a vydání.";
            text.manifest_payload = "Manifest JSON";
            text.dataset = "Datová sada";
            text.source_name = "Název zdroje";
            text.source_url = "URL zdroje";
            text.release_version = "Verze vydání";
            text.released_at = "Vydáno";
            text.retrieved_at = "Načteno";
            text.manifest_hash = "Hash manifestu";
            text.artifact_hash = "Hash artefaktu";
            text.parser_name = "Název parseru";
            text.parser_version = "Verze parseru";
            text.parser_revision = "Revize parseru";
            text.parser_configuration_hash = "Hash konfigurace parseru";
            text.record_license = "Licence záznamů";
            text.expected_chunks = "Očekávané části";
            text.expected_records = "Očekávané záznamy";
            text.create_batch = "Vytvořit dávku";
            text.upload_title = "Nahrát část";
            text.upload_hint =
                "Nahrajte do dávky každou očíslovanou část JSON. Shodné opakování je bezpečné.";
            text.batch_id = "ID dávky";
            text.chunk_number = "Číslo části";
            text.chunk_payload = "Část JSON";
            text.upload_chunk = "Nahrát část";
            text.finalize_title = "Ověřit dávku";
            text.finalize_hint = "Zkontrolujte úplnost, zpracujte každý záznam minerálu a vytvořte validační zprávu.";
            text.finalize_validate = "Dokončit a ověřit";
            text.batches_title = "Vydání dat o minerálech";
            text.empty = "Zatím nejsou žádná vydání dat o minerálech.";
            text.status = "Stav";
            text.status_receiving = "Přijímá data";
            text.status_ready = "Připraveno";
            text.status_needs_attention = "Vyžaduje pozornost";
            text.status_approved = "Schváleno";
            text.status_rejected = "Zamítnuto";
            text.progress = "Průběh";
            text.created_at = "Vytvořeno";
            text.finalized_at = "Dokončeno";
            text.uploaded_chunks = "Nahrané části";
            text.records = "Záznamy";
            text.created = "Vytvořeno";
            text.adopted = "Převzato";
            text.updated = "Aktualizováno";
            text.unchanged = "Beze změny";
            text.missing = "Chybí";
            text.blockers = "Blokující problémy";
            text.identity_warnings = "Upozornění k identitě";
            text.anomaly_samples = "Ukázky anomálií";
            text.no_anomalies = "Žádné ukázky anomálií.";
            text.review_samples = "Ukázky ke kontrole";
            text.source_record = "Zdrojový záznam";
            text.nomenclature_status = "Nomenklatorický stav";
            text.valid_species = "Platný druh";
            text.yes = "Ano";
            text.no = "Ne";
            text.report_hash = "Hash zprávy";
            text.base_batch = "Základní dávka";
            text.decision_title = "Rozhodnutí o vydání";
            text.decision_warning = "Schválením se toto vydání dat o minerálech zveřejní. Zamítnutím zůstane nedostupné. Než budete pokračovat, zkontrolujte zprávu.";
            text.acknowledge_warning =
                "Potvrzuji kontrolu validační zprávy a porozumění tomuto rozhodnutí.";
            text.confirmation = "Potvrzení";
            text.confirmation_hint = "Pro potvrzení zadejte přesné znění verze vydání.";
            text.operator_note = "Poznámka operátora";
            text.operator_note_placeholder =
                "Zaznamenejte podklady a odůvodnění tohoto rozhodnutí.";
            text.approve_release = "Schválit vydání";
            text.reject_release = "Zamítnout vydání";
            text.working = "Probíhá zpracování…";
            text.request_failed = "Požadavek selhal.";
            text.batch_created = "Dávka vytvořena.";
            text.chunk_uploaded = "Část nahrána.";
            text.validation_complete = "Ověření dokončeno.";
            text.approved_notice = "Vydání bylo schváleno a zveřejněno.";
            text.rejected_notice =
                "Vydání bylo zamítnuto; nebyly zveřejněny žádné záznamy minerálů.";
        }
        Language::De => {
            text.link = "Mineraldatenbank";
            text.title = "Mineraldatenbank";
            text.subtitle = "Offizielle Mineral-Datenversionen und ihr Veröffentlichungsstatus.";
            text.attribution_review = "Quelle";
            text.historical_attribution_missing =
                "Die Quellenangabe ist unvollständig. Diese Version kann nicht veröffentlicht werden.";
            text.back_to_admin = "Zurück zur Administration";
            text.individual_review_link = "Einzelne Mineralien prüfen";
            text.create_title = "Stapel erstellen";
            text.create_hint = "Vor dem Upload nummerierter Teile das vollständige Quellen- und Release-Manifest senden.";
            text.manifest_payload = "Manifest-JSON";
            text.dataset = "Datenbestand";
            text.source_name = "Quellenname";
            text.source_url = "Quell-URL";
            text.release_version = "Release-Version";
            text.released_at = "Veröffentlicht am";
            text.retrieved_at = "Abgerufen am";
            text.manifest_hash = "Manifest-Hash";
            text.artifact_hash = "Artefakt-Hash";
            text.parser_name = "Parser-Name";
            text.parser_version = "Parser-Version";
            text.parser_revision = "Parser-Revision";
            text.parser_configuration_hash = "Hash der Parser-Konfiguration";
            text.record_license = "Lizenz der Datensätze";
            text.expected_chunks = "Erwartete Teile";
            text.expected_records = "Erwartete Datensätze";
            text.create_batch = "Stapel erstellen";
            text.upload_title = "Teil hochladen";
            text.upload_hint = "Jeden nummerierten JSON-Teil in den Stapel hochladen. Identische Wiederholungen sind sicher.";
            text.batch_id = "Stapel-ID";
            text.chunk_number = "Teilnummer";
            text.chunk_payload = "JSON-Teil";
            text.upload_chunk = "Teil hochladen";
            text.finalize_title = "Stapel validieren";
            text.finalize_hint = "Vollständigkeit prüfen, jeden Mineral-Datensatz analysieren und den Validierungsbericht erstellen.";
            text.finalize_validate = "Abschließen und validieren";
            text.batches_title = "Mineral-Datenversionen";
            text.empty = "Noch keine Mineral-Datenversionen vorhanden.";
            text.status = "Status";
            text.status_receiving = "Wird empfangen";
            text.status_ready = "Bereit";
            text.status_needs_attention = "Prüfung erforderlich";
            text.status_approved = "Freigegeben";
            text.status_rejected = "Abgelehnt";
            text.progress = "Fortschritt";
            text.created_at = "Erstellt am";
            text.finalized_at = "Abgeschlossen am";
            text.uploaded_chunks = "Hochgeladene Teile";
            text.records = "Datensätze";
            text.created = "Erstellt";
            text.adopted = "Übernommen";
            text.updated = "Aktualisiert";
            text.unchanged = "Unverändert";
            text.missing = "Fehlend";
            text.blockers = "Blockierende Probleme";
            text.identity_warnings = "Identitätswarnungen";
            text.anomaly_samples = "Anomaliebeispiele";
            text.no_anomalies = "Keine Anomaliebeispiele.";
            text.review_samples = "Prüfbeispiele";
            text.source_record = "Quelldatensatz";
            text.nomenclature_status = "Nomenklaturstatus";
            text.valid_species = "Gültige Art";
            text.yes = "Ja";
            text.no = "Nein";
            text.report_hash = "Bericht-Hash";
            text.base_batch = "Basisstapel";
            text.decision_title = "Freigabeentscheidung";
            text.decision_warning = "Durch die Freigabe wird diese Mineral-Datenversion veröffentlicht. Bei Ablehnung bleibt sie unverfügbar. Prüfen Sie vor dem Fortfahren den Bericht.";
            text.acknowledge_warning = "Ich habe den Validierungsbericht geprüft und verstehe die Folgen dieser Entscheidung.";
            text.confirmation = "Bestätigung";
            text.confirmation_hint = "Zur Bestätigung die Release-Version exakt eingeben.";
            text.operator_note = "Operatornotiz";
            text.operator_note_placeholder =
                "Belege und Begründung für diese Entscheidung festhalten.";
            text.approve_release = "Datenversion freigeben";
            text.reject_release = "Datenversion ablehnen";
            text.working = "Wird verarbeitet…";
            text.request_failed = "Anfrage fehlgeschlagen.";
            text.batch_created = "Stapel erstellt.";
            text.chunk_uploaded = "Teil hochgeladen.";
            text.validation_complete = "Validierung abgeschlossen.";
            text.approved_notice = "Datenversion freigegeben und veröffentlicht.";
            text.rejected_notice =
                "Datenversion abgelehnt; es wurden keine Mineral-Datensätze veröffentlicht.";
        }
        Language::Fr => {
            text.link = "Base de données des minéraux";
            text.title = "Base de données des minéraux";
            text.subtitle =
                "Versions officielles des données minérales et état de leur publication.";
            text.attribution_review = "Source";
            text.historical_attribution_missing =
                "Les informations sur la source sont incomplètes. Cette version ne peut pas être publiée.";
            text.back_to_admin = "Retour à l’administration";
            text.individual_review_link = "Examiner les minéraux individuellement";
            text.create_title = "Créer un lot";
            text.create_hint = "Envoyez le manifeste complet de la source et de la version avant les fragments numérotés.";
            text.manifest_payload = "Manifeste JSON";
            text.dataset = "Jeu de données";
            text.source_name = "Nom de la source";
            text.source_url = "URL de la source";
            text.release_version = "Version de publication";
            text.released_at = "Publié le";
            text.retrieved_at = "Récupéré le";
            text.manifest_hash = "Hash du manifeste";
            text.artifact_hash = "Hash de l’artefact";
            text.parser_name = "Nom du parseur";
            text.parser_version = "Version du parseur";
            text.parser_revision = "Révision du parseur";
            text.parser_configuration_hash = "Hash de configuration du parseur";
            text.record_license = "Licence des enregistrements";
            text.expected_chunks = "Fragments attendus";
            text.expected_records = "Enregistrements attendus";
            text.create_batch = "Créer le lot";
            text.upload_title = "Téléverser un fragment";
            text.upload_hint = "Téléversez dans le lot chaque fragment JSON numéroté. Les reprises identiques sont sûres.";
            text.batch_id = "ID du lot";
            text.chunk_number = "Numéro du fragment";
            text.chunk_payload = "Fragment JSON";
            text.upload_chunk = "Téléverser le fragment";
            text.finalize_title = "Valider le lot";
            text.finalize_hint = "Vérifiez qu’il est complet, analysez chaque fiche minérale et générez le rapport de validation.";
            text.finalize_validate = "Finaliser et valider";
            text.batches_title = "Versions des données minérales";
            text.empty = "Aucune version des données minérales pour le moment.";
            text.status = "Statut";
            text.status_receiving = "Réception en cours";
            text.status_ready = "Prêt";
            text.status_needs_attention = "Attention requise";
            text.status_approved = "Approuvé";
            text.status_rejected = "Rejeté";
            text.progress = "Progression";
            text.created_at = "Créé le";
            text.finalized_at = "Finalisé le";
            text.uploaded_chunks = "Fragments téléversés";
            text.records = "Enregistrements";
            text.created = "Créés";
            text.adopted = "Adoptés";
            text.updated = "Mis à jour";
            text.unchanged = "Inchangés";
            text.missing = "Manquants";
            text.blockers = "Points bloquants";
            text.identity_warnings = "Avertissements d’identité";
            text.anomaly_samples = "Exemples d’anomalies";
            text.no_anomalies = "Aucun exemple d’anomalie.";
            text.review_samples = "Exemples à examiner";
            text.source_record = "Enregistrement source";
            text.nomenclature_status = "Statut nomenclatural";
            text.valid_species = "Espèce valide";
            text.yes = "Oui";
            text.no = "Non";
            text.report_hash = "Hash du rapport";
            text.base_batch = "Lot de base";
            text.decision_title = "Décision de publication";
            text.decision_warning = "L’approbation publie cette version des données minérales. Le rejet la maintient indisponible. Examinez le rapport avant de continuer.";
            text.acknowledge_warning =
                "J’ai examiné le rapport de validation et je comprends cette décision.";
            text.confirmation = "Confirmation";
            text.confirmation_hint =
                "Saisissez exactement la version de publication pour confirmer.";
            text.operator_note = "Note de l’opérateur";
            text.operator_note_placeholder =
                "Consignez les éléments probants et le raisonnement motivant cette décision.";
            text.approve_release = "Approuver la publication";
            text.reject_release = "Rejeter la publication";
            text.working = "Traitement en cours…";
            text.request_failed = "Échec de la requête.";
            text.batch_created = "Lot créé.";
            text.chunk_uploaded = "Fragment téléversé.";
            text.validation_complete = "Validation terminée.";
            text.approved_notice = "Version approuvée et publiée.";
            text.rejected_notice = "Version rejetée ; aucune fiche minérale n’a été publiée.";
        }
        Language::Pt => {
            text.link = "Base de dados de minerais";
            text.title = "Base de dados de minerais";
            text.subtitle = "Versões oficiais dos dados minerais e seu estado de publicação.";
            text.attribution_review = "Fonte";
            text.historical_attribution_missing =
                "As informações da fonte estão incompletas. Esta versão não pode ser publicada.";
            text.back_to_admin = "Voltar à administração";
            text.individual_review_link = "Revisar minerais individualmente";
            text.create_title = "Criar lote";
            text.create_hint =
                "Envie o manifesto completo da fonte e da versão antes das partes numeradas.";
            text.manifest_payload = "Manifesto JSON";
            text.dataset = "Conjunto de dados";
            text.source_name = "Nome da fonte";
            text.source_url = "URL da fonte";
            text.release_version = "Versão da publicação";
            text.released_at = "Publicado em";
            text.retrieved_at = "Obtido em";
            text.manifest_hash = "Hash do manifesto";
            text.artifact_hash = "Hash do artefato";
            text.parser_name = "Nome do parser";
            text.parser_version = "Versão do parser";
            text.parser_revision = "Revisão do parser";
            text.parser_configuration_hash = "Hash da configuração do parser";
            text.record_license = "Licença dos registros";
            text.expected_chunks = "Partes esperadas";
            text.expected_records = "Registros esperados";
            text.create_batch = "Criar lote";
            text.upload_title = "Enviar parte";
            text.upload_hint =
                "Envie ao lote cada parte JSON numerada. Repetições idênticas são seguras.";
            text.batch_id = "ID do lote";
            text.chunk_number = "Número da parte";
            text.chunk_payload = "Parte JSON";
            text.upload_chunk = "Enviar parte";
            text.finalize_title = "Validar lote";
            text.finalize_hint = "Verifique a completude, analise cada registro de mineral e gere o relatório de validação.";
            text.finalize_validate = "Finalizar e validar";
            text.batches_title = "Versões dos dados minerais";
            text.empty = "Ainda não há versões dos dados minerais.";
            text.status = "Status";
            text.status_receiving = "Recebendo";
            text.status_ready = "Pronto";
            text.status_needs_attention = "Requer atenção";
            text.status_approved = "Aprovado";
            text.status_rejected = "Rejeitado";
            text.progress = "Progresso";
            text.created_at = "Criado em";
            text.finalized_at = "Finalizado em";
            text.uploaded_chunks = "Partes enviadas";
            text.records = "Registros";
            text.created = "Criados";
            text.adopted = "Adotados";
            text.updated = "Atualizados";
            text.unchanged = "Sem alterações";
            text.missing = "Ausentes";
            text.blockers = "Bloqueios";
            text.identity_warnings = "Alertas de identidade";
            text.anomaly_samples = "Amostras de anomalias";
            text.no_anomalies = "Nenhuma amostra de anomalia.";
            text.review_samples = "Amostras para revisão";
            text.source_record = "Registro de origem";
            text.nomenclature_status = "Status nomenclatural";
            text.valid_species = "Espécie válida";
            text.yes = "Sim";
            text.no = "Não";
            text.report_hash = "Hash do relatório";
            text.base_batch = "Lote base";
            text.decision_title = "Decisão de publicação";
            text.decision_warning = "A aprovação publica esta versão dos dados de minerais. A rejeição a mantém indisponível. Revise o relatório antes de continuar.";
            text.acknowledge_warning =
                "Revisei o relatório de validação e compreendo esta decisão.";
            text.confirmation = "Confirmação";
            text.confirmation_hint = "Digite exatamente a versão da publicação para confirmar.";
            text.operator_note = "Nota do operador";
            text.operator_note_placeholder =
                "Registre as evidências e a justificativa para esta decisão.";
            text.approve_release = "Aprovar publicação";
            text.reject_release = "Rejeitar publicação";
            text.working = "Processando…";
            text.request_failed = "Falha na solicitação.";
            text.batch_created = "Lote criado.";
            text.chunk_uploaded = "Parte enviada.";
            text.validation_complete = "Validação concluída.";
            text.approved_notice = "Versão aprovada e publicada.";
            text.rejected_notice = "Versão rejeitada; nenhum registro de mineral foi publicado.";
        }
        Language::Zh => {
            text.link = "矿物数据库";
            text.title = "矿物数据库";
            text.subtitle = "官方矿物数据版本及其发布状态。";
            text.attribution_review = "来源";
            text.historical_attribution_missing = "来源信息不完整，无法发布此版本。";
            text.back_to_admin = "返回管理后台";
            text.individual_review_link = "逐条审核矿物";
            text.create_title = "创建批次";
            text.create_hint = "上传编号分块前，请提交完整的数据源和发布清单。";
            text.manifest_payload = "清单 JSON";
            text.dataset = "数据集";
            text.source_name = "数据源名称";
            text.source_url = "数据源 URL";
            text.release_version = "发布版本";
            text.released_at = "发布时间";
            text.retrieved_at = "获取时间";
            text.manifest_hash = "清单哈希";
            text.artifact_hash = "产物哈希";
            text.parser_name = "解析器名称";
            text.parser_version = "解析器版本";
            text.parser_revision = "解析器修订版";
            text.parser_configuration_hash = "解析器配置哈希";
            text.record_license = "记录许可证";
            text.expected_chunks = "预计分块数";
            text.expected_records = "预计记录数";
            text.create_batch = "创建批次";
            text.upload_title = "上传分块";
            text.upload_hint = "将每个带编号的 JSON 分块上传到该批次；内容完全相同的重试是安全的。";
            text.batch_id = "批次 ID";
            text.chunk_number = "分块编号";
            text.chunk_payload = "分块 JSON";
            text.upload_chunk = "上传分块";
            text.finalize_title = "验证批次";
            text.finalize_hint = "检查完整性，分析每条矿物记录，并生成验证报告。";
            text.finalize_validate = "完成并验证";
            text.batches_title = "矿物数据版本";
            text.empty = "暂无矿物数据版本。";
            text.status = "状态";
            text.status_receiving = "接收中";
            text.status_ready = "就绪";
            text.status_needs_attention = "需要处理";
            text.status_approved = "已批准";
            text.status_rejected = "已拒绝";
            text.progress = "进度";
            text.created_at = "创建时间";
            text.finalized_at = "完成时间";
            text.uploaded_chunks = "已上传分块";
            text.records = "记录";
            text.created = "已创建";
            text.adopted = "已采用";
            text.updated = "已更新";
            text.unchanged = "未更改";
            text.missing = "缺失";
            text.blockers = "阻断项";
            text.identity_warnings = "鉴定警告";
            text.anomaly_samples = "异常样本";
            text.no_anomalies = "无异常样本。";
            text.review_samples = "审核样本";
            text.source_record = "源记录";
            text.nomenclature_status = "命名状态";
            text.valid_species = "有效矿物种";
            text.yes = "是";
            text.no = "否";
            text.report_hash = "报告哈希";
            text.base_batch = "基础批次";
            text.decision_title = "发布决定";
            text.decision_warning =
                "批准后将发布此矿物数据版本；拒绝后该版本仍不可用。继续前请审核报告。";
            text.acknowledge_warning = "我已审核验证报告并理解此决定。";
            text.confirmation = "确认";
            text.confirmation_hint = "请输入完全一致的发布版本以确认。";
            text.operator_note = "操作员备注";
            text.operator_note_placeholder = "记录此决定的证据和理由。";
            text.approve_release = "批准发布";
            text.reject_release = "拒绝发布";
            text.working = "处理中…";
            text.request_failed = "请求失败。";
            text.batch_created = "批次已创建。";
            text.chunk_uploaded = "分块已上传。";
            text.validation_complete = "验证完成。";
            text.approved_notice = "版本已批准并发布。";
            text.rejected_notice = "版本已拒绝；未发布任何矿物记录。";
        }
        Language::Ja => {
            text.link = "鉱物データベース";
            text.title = "鉱物データベース";
            text.subtitle = "公式の鉱物データリリースと公開状況。";
            text.attribution_review = "情報源";
            text.historical_attribution_missing =
                "情報源の表記が不完全なため、このリリースは公開できません。";
            text.back_to_admin = "管理画面に戻る";
            text.individual_review_link = "鉱物を個別に確認";
            text.create_title = "バッチを作成";
            text.create_hint = "番号付きチャンクをアップロードする前に、データソースとリリースの完全なマニフェストを送信してください。";
            text.manifest_payload = "マニフェスト JSON";
            text.dataset = "データセット";
            text.source_name = "データソース名";
            text.source_url = "データソース URL";
            text.release_version = "リリースバージョン";
            text.released_at = "リリース日時";
            text.retrieved_at = "取得日時";
            text.manifest_hash = "マニフェストのハッシュ";
            text.artifact_hash = "成果物のハッシュ";
            text.parser_name = "パーサー名";
            text.parser_version = "パーサーバージョン";
            text.parser_revision = "パーサーリビジョン";
            text.parser_configuration_hash = "パーサー設定のハッシュ";
            text.record_license = "レコードのライセンス";
            text.expected_chunks = "予定チャンク数";
            text.expected_records = "予定レコード数";
            text.create_batch = "バッチを作成";
            text.upload_title = "チャンクをアップロード";
            text.upload_hint = "番号付きの各 JSON チャンクをバッチにアップロードします。同一内容の再試行は安全です。";
            text.batch_id = "バッチ ID";
            text.chunk_number = "チャンク番号";
            text.chunk_payload = "チャンク JSON";
            text.upload_chunk = "チャンクをアップロード";
            text.finalize_title = "バッチを検証";
            text.finalize_hint =
                "完全性を確認し、すべての鉱物レコードを分析して検証レポートを作成します。";
            text.finalize_validate = "確定して検証";
            text.batches_title = "鉱物データリリース";
            text.empty = "鉱物データリリースはまだありません。";
            text.status = "ステータス";
            text.status_receiving = "受信中";
            text.status_ready = "準備完了";
            text.status_needs_attention = "要確認";
            text.status_approved = "承認済み";
            text.status_rejected = "却下済み";
            text.progress = "進捗";
            text.created_at = "作成日時";
            text.finalized_at = "確定日時";
            text.uploaded_chunks = "アップロード済みチャンク";
            text.records = "レコード";
            text.created = "作成";
            text.adopted = "採用";
            text.updated = "更新";
            text.unchanged = "変更なし";
            text.missing = "不足";
            text.blockers = "ブロッカー";
            text.identity_warnings = "同定に関する警告";
            text.anomaly_samples = "異常サンプル";
            text.no_anomalies = "異常サンプルはありません。";
            text.review_samples = "確認サンプル";
            text.source_record = "ソースレコード";
            text.nomenclature_status = "命名ステータス";
            text.valid_species = "有効な鉱物種";
            text.yes = "はい";
            text.no = "いいえ";
            text.report_hash = "レポートのハッシュ";
            text.base_batch = "ベースバッチ";
            text.decision_title = "リリース判定";
            text.decision_warning = "承認するとこの鉱物データリリースが公開されます。却下した場合は利用できないままです。続行する前に検証レポートを確認してください。";
            text.acknowledge_warning = "検証レポートを確認し、この判断の内容を理解しました。";
            text.confirmation = "確認";
            text.confirmation_hint = "確認のため、リリースバージョンを正確に入力してください。";
            text.operator_note = "オペレーター注記";
            text.operator_note_placeholder = "この判断の根拠と理由を記録してください。";
            text.approve_release = "リリースを承認";
            text.reject_release = "リリースを却下";
            text.working = "処理中…";
            text.request_failed = "リクエストに失敗しました。";
            text.batch_created = "バッチを作成しました。";
            text.chunk_uploaded = "チャンクをアップロードしました。";
            text.validation_complete = "検証が完了しました。";
            text.approved_notice = "リリースを承認し、公開しました。";
            text.rejected_notice = "リリースを却下しました。鉱物レコードは公開されていません。";
        }
        Language::Ar => {
            text.link = "قاعدة بيانات المعادن";
            text.title = "قاعدة بيانات المعادن";
            text.subtitle = "الإصدارات الرسمية لبيانات المعادن وحالة نشرها.";
            text.attribution_review = "المصدر";
            text.historical_attribution_missing =
                "معلومات المصدر غير مكتملة، لذلك لا يمكن نشر هذا الإصدار.";
            text.back_to_admin = "العودة إلى الإدارة";
            text.individual_review_link = "مراجعة المعادن كلًّا على حدة";
            text.create_title = "إنشاء دفعة";
            text.create_hint = "أرسل البيان الكامل للمصدر والإصدار قبل تحميل الأجزاء المرقّمة.";
            text.manifest_payload = "ملف البيان بصيغة JSON";
            text.dataset = "مجموعة البيانات";
            text.source_name = "اسم المصدر";
            text.source_url = "عنوان URL للمصدر";
            text.release_version = "نسخة الإصدار";
            text.released_at = "وقت الإصدار";
            text.retrieved_at = "وقت الاسترجاع";
            text.manifest_hash = "تجزئة ملف البيان";
            text.artifact_hash = "تجزئة الملف الناتج";
            text.parser_name = "اسم المحلّل";
            text.parser_version = "إصدار المحلّل";
            text.parser_revision = "رقم مراجعة المحلّل";
            text.parser_configuration_hash = "تجزئة إعدادات المحلّل";
            text.record_license = "ترخيص السجلات";
            text.expected_chunks = "عدد الأجزاء المتوقع";
            text.expected_records = "عدد السجلات المتوقع";
            text.create_batch = "إنشاء الدفعة";
            text.upload_title = "تحميل جزء";
            text.upload_hint =
                "حمّل كل جزء JSON مرقّم إلى الدفعة. إعادة المحاولة بالمحتوى نفسه آمنة.";
            text.batch_id = "معرّف الدفعة";
            text.chunk_number = "رقم الجزء";
            text.chunk_payload = "محتوى الجزء بصيغة JSON";
            text.upload_chunk = "تحميل الجزء";
            text.finalize_title = "التحقق من الدفعة";
            text.finalize_hint = "تحقق من الاكتمال، وحلّل كل سجل معدن، وأنشئ تقرير التحقق.";
            text.finalize_validate = "إنهاء الدفعة والتحقق منها";
            text.batches_title = "إصدارات بيانات المعادن";
            text.empty = "لا توجد إصدارات لبيانات المعادن بعد.";
            text.status = "الحالة";
            text.status_receiving = "جارٍ الاستلام";
            text.status_ready = "جاهزة";
            text.status_needs_attention = "تتطلب الانتباه";
            text.status_approved = "معتمدة";
            text.status_rejected = "مرفوضة";
            text.progress = "التقدم";
            text.created_at = "وقت الإنشاء";
            text.finalized_at = "وقت الإنهاء";
            text.uploaded_chunks = "الأجزاء المحمّلة";
            text.records = "السجلات";
            text.created = "مُنشأة";
            text.adopted = "مُعتمدة";
            text.updated = "محدّثة";
            text.unchanged = "دون تغيير";
            text.missing = "مفقودة";
            text.blockers = "العوائق";
            text.identity_warnings = "تحذيرات تحديد الهوية";
            text.anomaly_samples = "عيّنات الشذوذ";
            text.no_anomalies = "لا توجد عيّنات شذوذ.";
            text.review_samples = "عيّنات للمراجعة";
            text.source_record = "السجل المصدر";
            text.nomenclature_status = "حالة التسمية";
            text.valid_species = "نوع معدني معتمد";
            text.yes = "نعم";
            text.no = "لا";
            text.report_hash = "تجزئة التقرير";
            text.base_batch = "الدفعة الأساس";
            text.decision_title = "قرار النشر";
            text.decision_warning = "سيؤدي الاعتماد إلى نشر هذا الإصدار من بيانات المعادن، أما الرفض فسيبقيه غير متاح. راجع تقرير التحقق قبل المتابعة.";
            text.acknowledge_warning = "راجعت تقرير التحقق وأفهم تبعات هذا القرار.";
            text.confirmation = "التأكيد";
            text.confirmation_hint = "أدخل نسخة الإصدار كما هي تمامًا للتأكيد.";
            text.operator_note = "ملاحظة المشغّل";
            text.operator_note_placeholder = "سجّل الأدلة والمسوغات التي يستند إليها هذا القرار.";
            text.approve_release = "اعتماد الإصدار";
            text.reject_release = "رفض الإصدار";
            text.working = "جارٍ التنفيذ…";
            text.request_failed = "فشل الطلب.";
            text.batch_created = "تم إنشاء الدفعة.";
            text.chunk_uploaded = "تم تحميل الجزء.";
            text.validation_complete = "اكتمل التحقق.";
            text.approved_notice = "تم اعتماد الإصدار ونشره.";
            text.rejected_notice = "تم رفض الإصدار؛ لم تُنشر أي سجلات للمعادن.";
        }
        Language::Hi => {
            text.link = "खनिज डेटाबेस";
            text.title = "खनिज डेटाबेस";
            text.subtitle = "आधिकारिक खनिज डेटा रिलीज़ और उनके प्रकाशन की स्थिति।";
            text.attribution_review = "स्रोत";
            text.historical_attribution_missing =
                "स्रोत की जानकारी अधूरी है। यह रिलीज़ प्रकाशित नहीं की जा सकती।";
            text.back_to_admin = "एडमिन पर वापस जाएँ";
            text.individual_review_link = "एक-एक खनिज की समीक्षा करें";
            text.create_title = "बैच बनाएँ";
            text.create_hint = "क्रमांकित चंक अपलोड करने से पहले स्रोत और रिलीज़ का पूरा मैनिफ़ेस्ट भेजें।";
            text.manifest_payload = "मैनिफ़ेस्ट JSON";
            text.dataset = "डेटासेट";
            text.source_name = "स्रोत का नाम";
            text.source_url = "स्रोत URL";
            text.release_version = "रिलीज़ संस्करण";
            text.released_at = "रिलीज़ का समय";
            text.retrieved_at = "प्राप्ति का समय";
            text.manifest_hash = "मैनिफ़ेस्ट हैश";
            text.artifact_hash = "आर्टिफ़ैक्ट हैश";
            text.parser_name = "पार्सर का नाम";
            text.parser_version = "पार्सर संस्करण";
            text.parser_revision = "पार्सर संशोधन";
            text.parser_configuration_hash = "पार्सर कॉन्फ़िगरेशन हैश";
            text.record_license = "रिकॉर्ड लाइसेंस";
            text.expected_chunks = "अपेक्षित चंक";
            text.expected_records = "अपेक्षित रिकॉर्ड";
            text.create_batch = "बैच बनाएँ";
            text.upload_title = "चंक अपलोड करें";
            text.upload_hint =
                "हर क्रमांकित JSON चंक को बैच में अपलोड करें। समान सामग्री वाली पुनः कोशिश सुरक्षित है।";
            text.batch_id = "बैच ID";
            text.chunk_number = "चंक संख्या";
            text.chunk_payload = "चंक JSON";
            text.upload_chunk = "चंक अपलोड करें";
            text.finalize_title = "बैच सत्यापित करें";
            text.finalize_hint = "पूर्णता जाँचें, हर खनिज रिकॉर्ड का विश्लेषण करें और सत्यापन रिपोर्ट बनाएँ।";
            text.finalize_validate = "पूर्ण करके सत्यापित करें";
            text.batches_title = "खनिज डेटा रिलीज़";
            text.empty = "अभी कोई खनिज डेटा रिलीज़ नहीं है।";
            text.status = "स्थिति";
            text.status_receiving = "प्राप्त हो रहा है";
            text.status_ready = "तैयार";
            text.status_needs_attention = "ध्यान आवश्यक";
            text.status_approved = "अनुमोदित";
            text.status_rejected = "अस्वीकृत";
            text.progress = "प्रगति";
            text.created_at = "बनाने का समय";
            text.finalized_at = "अंतिम रूप देने का समय";
            text.uploaded_chunks = "अपलोड किए गए चंक";
            text.records = "रिकॉर्ड";
            text.created = "बनाए गए";
            text.adopted = "अपनाए गए";
            text.updated = "अपडेट किए गए";
            text.unchanged = "अपरिवर्तित";
            text.missing = "गुम";
            text.blockers = "अवरोधक";
            text.identity_warnings = "पहचान संबंधी चेतावनियाँ";
            text.anomaly_samples = "विसंगति के नमूने";
            text.no_anomalies = "कोई विसंगति नमूना नहीं है।";
            text.review_samples = "समीक्षा नमूने";
            text.source_record = "स्रोत रिकॉर्ड";
            text.nomenclature_status = "नामकरण स्थिति";
            text.valid_species = "मान्य खनिज प्रजाति";
            text.yes = "हाँ";
            text.no = "नहीं";
            text.report_hash = "रिपोर्ट हैश";
            text.base_batch = "आधार बैच";
            text.decision_title = "रिलीज़ निर्णय";
            text.decision_warning = "अनुमोदन से यह खनिज डेटा रिलीज़ प्रकाशित हो जाएगी। अस्वीकृति पर यह अनुपलब्ध रहेगी। आगे बढ़ने से पहले सत्यापन रिपोर्ट की समीक्षा करें।";
            text.acknowledge_warning =
                "सत्यापन रिपोर्ट की समीक्षा कर ली गई है और इस निर्णय के प्रभाव समझ लिए गए हैं।";
            text.confirmation = "पुष्टि";
            text.confirmation_hint = "पुष्टि के लिए रिलीज़ संस्करण को ठीक उसी तरह दर्ज करें।";
            text.operator_note = "ऑपरेटर टिप्पणी";
            text.operator_note_placeholder = "इस निर्णय के प्रमाण और तर्क दर्ज करें।";
            text.approve_release = "रिलीज़ अनुमोदित करें";
            text.reject_release = "रिलीज़ अस्वीकार करें";
            text.working = "कार्य जारी है…";
            text.request_failed = "अनुरोध विफल रहा।";
            text.batch_created = "बैच बन गया।";
            text.chunk_uploaded = "चंक अपलोड हो गया।";
            text.validation_complete = "सत्यापन पूरा हुआ।";
            text.approved_notice = "रिलीज़ अनुमोदित और प्रकाशित हो गई।";
            text.rejected_notice = "रिलीज़ अस्वीकार कर दी गई; कोई खनिज रिकॉर्ड प्रकाशित नहीं हुआ।";
        }
    }

    text
}

fn en_text() -> UiText {
    UiText {
        registry: registry_en(),
        review: review_en(),
        ingestion: ingestion_text(Language::En),
        nav_home: "Home",
        nav_all_minerals: "All Minerals",
        nav_about: "About",
        nav_admin: "Admin",
        nav_login: "login",
        nav_current_mineral: "Current Mineral",
        nav_report: "Report",
        session_admin_active: "Admin session active",
        session_public_mode: "Public mode",
        session_secure_active: "Secure session active",
        session_auth_required: "Authentication required",

        home_title: "Minerals",
        home_subtitle: "Select your language and continue to the mineral sections.",
        home_select_language: "Language",
        home_continue: "Continue",

        catalog_title: "Minerals Catalog",
        catalog_subtitle: "Explore published mineral profiles with clear physical, chemical, and provenance information.",
        no_minerals: "No mineral profiles are available yet.",
        open_mineral: "Open Mineral",
        all_minerals_title: "Minerals of the World",
        all_minerals_subtitle: "Discover mineral species from around the world and follow our growing catalog of researched profiles.",
        all_minerals_published_label: "Profiles available now",
        all_minerals_estimated_label: "Known mineral species worldwide",
        all_minerals_disclaimer: "Coverage grows as records gain reliable sources and review. Unconfirmed information is kept clearly marked.",

        label_family: "Family",
        label_formula: "Formula",
        label_hardness: "Hardness (Mohs)",
        label_density: "Density (g/cm3)",
        label_description: "Description",
        label_crystal_system: "Crystal System",
        label_color: "Color",
        label_streak: "Streak",
        label_luster: "Luster",
        label_notes: "Notes",
        label_hardness_band: "Hardness Band",
        label_density_band: "Density Band",
        label_dominant_element: "Dominant Element",
        label_audience: "Audience",
        label_purpose: "Purpose",
        label_site_context: "Site Context",
        label_generated_utc: "Generated (UTC)",
        label_weight_pct: "Weight Percent",

        mineral_profile: "Mineral Profile",
        major_composition: "Major Chemical Composition",
        computed_classification: "Computed Classification",
        report_builder: "Report Builder",
        report_builder_subtitle: "Create a downloadable report for this mineral profile.",
        generate_pdf: "Generate PDF",
        status_pdf: "PDF",
        status_html: "HTML",
        status_pdf_failed: "PDF generation failed.",
        current_chain_output: "Research summary",
        recommendations_heading: "Recommendations",

        about_title: "About Minerals",
        about_subtitle: "An open knowledge and sourcing platform for trustworthy information about minerals.",
        about_operating_model: "Our approach",
        about_operating_body: "We connect scientific identity, cited evidence, and real-world availability while keeping research findings separate from seller claims.",
        about_path_note: "Every scientific claim should be traceable to a source, and every record should become more reliable as evidence and review improve.",

        footer_contact: "Contact",
        footer_legal: "Legal",
        footer_mission: "Mission",
        footer_contact_us: "contact us",
        footer_support: "support",
        footer_work_with_us: "work with us",
        footer_account: "account",
        footer_legal_link: "legal",
        footer_privacy_policy: "privacy policy",
        footer_terms_of_service: "terms of service",
        footer_returns_and_refunds: "returns and refunds",
        footer_shipping: "shipping",
        footer_about_us: "about us",
        footer_conflict_free_minerals: "conflict free minerals",
        footer_faq: "frequently asked questions",
        footer_powered_trust_by: "powered trust by",

        report_title_suffix: "Mineral Report",
        context_heading: "Context",
        snapshot_heading: "Physical and Chemical Snapshot",
        summary_heading: "Interpretive Summary",
        major_elements_heading: "Major Elements",
        notes_heading: "Notes",
    }
}

pub fn ui_text(lang: Language) -> UiText {
    debug_assert!(material_fact_label(lang, "appearance").is_some());
    let mut t = en_text();
    t.ingestion = ingestion_text(lang);

    match lang {
        Language::En => {}
        Language::Es => {
            t.registry = registry_es();
            t.review = review_es();
            t.nav_home = "Inicio";
            t.nav_all_minerals = "Todos los minerales";
            t.nav_about = "Acerca de";
            t.nav_admin = "Admin";
            t.nav_login = "iniciar sesión";
            t.nav_current_mineral = "Mineral actual";
            t.nav_report = "Informe";
            t.session_admin_active = "Sesión de admin activa";
            t.session_public_mode = "Modo público";
            t.session_secure_active = "Sesión segura activa";
            t.session_auth_required = "Autenticación requerida";
            t.home_title = "Minerales";
            t.home_subtitle = "Selecciona tu idioma y continúa al catálogo de minerales.";
            t.home_select_language = "Idioma";
            t.home_continue = "Continuar";
            t.catalog_title = "Catálogo de minerales";
            t.catalog_subtitle = "Explora perfiles de minerales con información física, química y de procedencia claramente presentada.";
            t.no_minerals = "Aún no hay perfiles de minerales disponibles.";
            t.open_mineral = "Abrir mineral";
            t.all_minerals_title = "Minerales del mundo";
            t.all_minerals_subtitle = "Descubre especies minerales de todo el mundo y sigue nuestro catálogo creciente de perfiles investigados.";
            t.all_minerals_published_label = "Perfiles disponibles ahora";
            t.all_minerals_estimated_label = "Especies minerales conocidas en el mundo";
            t.all_minerals_disclaimer = "La cobertura crece a medida que los registros incorporan fuentes fiables y revisión. La información sin confirmar permanece claramente indicada.";
            t.label_family = "Familia";
            t.label_description = "Descripción";
            t.label_crystal_system = "Sistema cristalino";
            t.label_color = "Color";
            t.label_streak = "Raya";
            t.label_luster = "Brillo";
            t.label_notes = "Notas";
            t.label_hardness_band = "Banda de dureza";
            t.label_density_band = "Banda de densidad";
            t.label_dominant_element = "Elemento dominante";
            t.label_purpose = "Propósito";
            t.label_site_context = "Contexto del sitio";
            t.mineral_profile = "Perfil del mineral";
            t.major_composition = "Composición química principal";
            t.computed_classification = "Clasificación calculada";
            t.report_builder = "Generador de informes";
            t.report_builder_subtitle = "Crea un informe descargable para este perfil mineral.";
            t.generate_pdf = "Generar PDF";
            t.status_pdf_failed = "Falló la generación de PDF.";
            t.current_chain_output = "Resumen de investigación";
            t.recommendations_heading = "Recomendaciones";
            t.about_title = "Acerca de Minerals";
            t.about_subtitle = "Una plataforma abierta de conocimiento y abastecimiento para obtener información fiable sobre minerales.";
            t.about_operating_model = "Nuestro enfoque";
            t.about_operating_body = "Conectamos la identidad científica, la evidencia citada y la disponibilidad real, manteniendo separados los hallazgos de investigación y las afirmaciones comerciales.";
            t.about_path_note = "Cada afirmación científica debe poder rastrearse hasta una fuente, y cada registro debe ganar fiabilidad a medida que mejoran la evidencia y la revisión.";
            t.footer_contact = "Contacto";
            t.footer_legal = "Legal";
            t.footer_mission = "Misión";
            t.footer_contact_us = "contáctanos";
            t.footer_support = "soporte";
            t.footer_work_with_us = "trabaja con nosotros";
            t.footer_account = "cuenta";
            t.footer_legal_link = "aviso legal";
            t.footer_privacy_policy = "política de privacidad";
            t.footer_terms_of_service = "términos de servicio";
            t.footer_returns_and_refunds = "devoluciones y reembolsos";
            t.footer_shipping = "envíos";
            t.footer_about_us = "sobre nosotros";
            t.footer_conflict_free_minerals = "minerales libres de conflicto";
            t.footer_faq = "preguntas frecuentes";
            t.footer_powered_trust_by = "impulsado por";
            t.report_title_suffix = "Informe mineral";
            t.context_heading = "Contexto";
            t.snapshot_heading = "Resumen físico y químico";
            t.summary_heading = "Resumen interpretativo";
            t.major_elements_heading = "Elementos principales";
        }
        Language::Cs => {
            t.registry = registry_cs();
            t.review = review_cs();
            t.nav_home = "Domů";
            t.nav_all_minerals = "Všechny minerály";
            t.nav_about = "O aplikaci";
            t.nav_login = "přihlásit se";
            t.session_public_mode = "Veřejný režim";
            t.home_title = "Minerály";
            t.home_subtitle = "Vyberte jazyk a pokračujte do katalogu minerálů.";
            t.home_select_language = "Jazyk";
            t.home_continue = "Pokračovat";
            t.catalog_title = "Katalog minerálů";
            t.catalog_subtitle = "Prohlédněte si profily minerálů s přehlednými fyzikálními, chemickými a původovými údaji.";
            t.no_minerals = "Zatím nejsou k dispozici žádné profily minerálů.";
            t.open_mineral = "Otevřít minerál";
            t.all_minerals_title = "Minerály světa";
            t.all_minerals_subtitle = "Objevujte minerální druhy z celého světa a sledujte náš rostoucí katalog zpracovaných profilů.";
            t.all_minerals_published_label = "Nyní dostupné profily";
            t.all_minerals_estimated_label = "Známé minerální druhy na světě";
            t.all_minerals_disclaimer = "Rozsah roste s přibývajícími spolehlivými zdroji a odbornou kontrolou. Nepotvrzené údaje jsou zřetelně označeny.";
            t.label_family = "Skupina";
            t.label_description = "Popis";
            t.label_crystal_system = "Krystalová soustava";
            t.label_notes = "Poznámky";
            t.mineral_profile = "Profil minerálu";
            t.major_composition = "Hlavní chemické složení";
            t.computed_classification = "Vypočtená klasifikace";
            t.report_builder = "Generátor reportu";
            t.generate_pdf = "Vygenerovat PDF";
            t.status_pdf_failed = "Generování PDF selhalo.";
            t.current_chain_output = "Shrnutí výzkumu";
            t.recommendations_heading = "Doporučení";
            t.about_title = "O Minerals";
            t.about_subtitle =
                "Otevřená platforma znalostí a zásobování pro spolehlivé informace o minerálech.";
            t.about_operating_model = "Náš přístup";
            t.about_operating_body = "Propojujeme vědeckou identitu, citované podklady a skutečnou dostupnost a přitom oddělujeme výsledky výzkumu od tvrzení prodejců.";
            t.about_path_note = "Každé vědecké tvrzení má být dohledatelné ke zdroji a každý záznam má být spolehlivější s lepšími podklady a kontrolou.";
            t.footer_contact = "Kontakt";
            t.footer_legal = "Právní";
            t.footer_mission = "Mise";
            t.footer_contact_us = "kontaktujte nás";
            t.footer_support = "podpora";
            t.footer_work_with_us = "pracujte s námi";
            t.footer_account = "účet";
            t.footer_legal_link = "právní informace";
            t.footer_privacy_policy = "zásady ochrany osobních údajů";
            t.footer_terms_of_service = "podmínky služby";
            t.footer_returns_and_refunds = "vrácení a refundace";
            t.footer_shipping = "doprava";
            t.footer_about_us = "o nás";
            t.footer_conflict_free_minerals = "minerály bez konfliktu";
            t.footer_faq = "často kladené dotazy";
            t.footer_powered_trust_by = "s důvěrou provozuje";
            t.report_title_suffix = "Report minerálu";
            t.context_heading = "Kontext";
            t.snapshot_heading = "Fyzikální a chemický přehled";
            t.summary_heading = "Interpretace";
            t.major_elements_heading = "Hlavní prvky";
        }
        Language::Zh => {
            t.registry = registry_zh();
            t.review = review_zh();
            t.nav_home = "首页";
            t.nav_all_minerals = "全部矿物";
            t.nav_about = "关于";
            t.nav_admin = "管理";
            t.nav_login = "登录";
            t.nav_current_mineral = "当前矿物";
            t.nav_report = "报告";
            t.session_admin_active = "管理员会话已启用";
            t.session_public_mode = "公开模式";
            t.session_secure_active = "安全会话已启用";
            t.session_auth_required = "需要认证";
            t.home_title = "矿物系统";
            t.home_subtitle = "选择语言并进入矿物目录。";
            t.home_select_language = "语言";
            t.home_continue = "继续";
            t.catalog_title = "矿物目录";
            t.catalog_subtitle = "浏览矿物档案，清晰了解物理、化学和来源信息。";
            t.no_minerals = "目前还没有可用的矿物档案。";
            t.open_mineral = "打开矿物";
            t.all_minerals_title = "世界矿物";
            t.all_minerals_subtitle = "发现世界各地的矿物种类，并关注不断扩展的研究档案。";
            t.all_minerals_published_label = "当前可用档案";
            t.all_minerals_estimated_label = "全球已知矿物种类";
            t.all_minerals_disclaimer =
                "随着可靠来源和审核不断增加，收录范围也会扩大。未经确认的信息会被清楚标示。";
            t.label_family = "族";
            t.label_formula = "化学式";
            t.label_hardness = "硬度 (Mohs)";
            t.label_density = "密度 (g/cm3)";
            t.label_description = "描述";
            t.label_crystal_system = "晶系";
            t.label_color = "颜色";
            t.label_streak = "条痕";
            t.label_luster = "光泽";
            t.label_notes = "备注";
            t.label_hardness_band = "硬度等级";
            t.label_density_band = "密度等级";
            t.label_dominant_element = "主导元素";
            t.label_audience = "受众";
            t.label_purpose = "目的";
            t.label_site_context = "现场背景";
            t.label_generated_utc = "生成时间 (UTC)";
            t.label_weight_pct = "质量百分比";
            t.mineral_profile = "矿物概况";
            t.major_composition = "主要化学组成";
            t.computed_classification = "计算分类";
            t.report_builder = "报告生成";
            t.report_builder_subtitle = "为此矿物档案创建可下载的报告。";
            t.generate_pdf = "生成 PDF";
            t.status_pdf = "PDF";
            t.status_html = "HTML";
            t.status_pdf_failed = "PDF 生成失败。";
            t.current_chain_output = "研究摘要";
            t.recommendations_heading = "建议";
            t.about_title = "关于 Minerals";
            t.about_subtitle = "面向矿物可信信息的开放知识与采购平台。";
            t.about_operating_model = "我们的方法";
            t.about_operating_body =
                "我们连接科学身份、引用依据与现实供应情况，并将研究结论和供应商信息分开呈现。";
            t.about_path_note =
                "每项科学主张都应能追溯到来源；随着依据和审核的完善，每份档案都应变得更加可靠。";
            t.footer_contact = "联系";
            t.footer_legal = "法律";
            t.footer_mission = "使命";
            t.footer_contact_us = "联系我们";
            t.footer_support = "支持";
            t.footer_work_with_us = "与我们合作";
            t.footer_account = "账户";
            t.footer_legal_link = "法律声明";
            t.footer_privacy_policy = "隐私政策";
            t.footer_terms_of_service = "服务条款";
            t.footer_returns_and_refunds = "退货与退款";
            t.footer_shipping = "配送";
            t.footer_about_us = "关于我们";
            t.footer_conflict_free_minerals = "无冲突矿产";
            t.footer_faq = "常见问题";
            t.footer_powered_trust_by = "技术支持";
            t.report_title_suffix = "矿物报告";
            t.context_heading = "上下文";
            t.snapshot_heading = "物理与化学概览";
            t.summary_heading = "解释性总结";
            t.major_elements_heading = "主要元素";
            t.notes_heading = "备注";
        }
        Language::Ar => {
            t.registry = registry_ar();
            t.review = review_ar();
            t.nav_home = "الرئيسية";
            t.nav_all_minerals = "كل المعادن";
            t.nav_about = "حول";
            t.nav_admin = "الإدارة";
            t.nav_login = "تسجيل الدخول";
            t.nav_current_mineral = "المعدن الحالي";
            t.nav_report = "تقرير";
            t.session_admin_active = "جلسة الإدارة نشطة";
            t.session_public_mode = "وضع عام";
            t.session_secure_active = "جلسة آمنة نشطة";
            t.session_auth_required = "المصادقة مطلوبة";
            t.home_title = "المعادن";
            t.home_subtitle = "اختر اللغة ثم تابع إلى فهرس المعادن.";
            t.home_select_language = "اللغة";
            t.home_continue = "متابعة";
            t.catalog_title = "فهرس المعادن";
            t.catalog_subtitle =
                "استكشف ملفات المعادن مع معلومات واضحة عن خصائصها الفيزيائية والكيميائية ومصدرها.";
            t.no_minerals = "لا تتوفر ملفات معادن حالياً.";
            t.open_mineral = "فتح المعدن";
            t.all_minerals_title = "معادن العالم";
            t.all_minerals_subtitle =
                "اكتشف أنواع المعادن من أنحاء العالم وتابع فهرسنا المتنامي من الملفات المدروسة.";
            t.all_minerals_published_label = "الملفات المتاحة الآن";
            t.all_minerals_estimated_label = "أنواع المعادن المعروفة عالمياً";
            t.all_minerals_disclaimer = "يتوسع نطاق التغطية مع إضافة المصادر الموثوقة والمراجعة. وتبقى المعلومات غير المؤكدة مميزة بوضوح.";
            t.label_family = "العائلة";
            t.label_formula = "الصيغة";
            t.label_hardness = "الصلادة (موهس)";
            t.label_density = "الكثافة (g/cm3)";
            t.label_description = "الوصف";
            t.label_crystal_system = "النظام البلوري";
            t.label_color = "اللون";
            t.label_streak = "المخدش";
            t.label_luster = "البريق";
            t.label_notes = "ملاحظات";
            t.label_hardness_band = "فئة الصلادة";
            t.label_density_band = "فئة الكثافة";
            t.label_dominant_element = "العنصر الغالب";
            t.label_audience = "الجمهور";
            t.label_purpose = "الغرض";
            t.label_site_context = "سياق الموقع";
            t.label_generated_utc = "وقت الإنشاء (UTC)";
            t.label_weight_pct = "النسبة الوزنية";
            t.mineral_profile = "ملف المعدن";
            t.major_composition = "التركيب الكيميائي الرئيسي";
            t.computed_classification = "التصنيف المحسوب";
            t.report_builder = "منشئ التقرير";
            t.report_builder_subtitle = "أنشئ تقريراً قابلاً للتنزيل لملف هذا المعدن.";
            t.generate_pdf = "إنشاء PDF";
            t.status_pdf = "PDF";
            t.status_html = "HTML";
            t.status_pdf_failed = "فشل إنشاء PDF.";
            t.current_chain_output = "ملخص البحث";
            t.recommendations_heading = "التوصيات";
            t.about_title = "حول Minerals";
            t.about_subtitle = "منصة معرفة وتوريد مفتوحة لمعلومات موثوقة عن المعادن.";
            t.about_operating_model = "نهجنا";
            t.about_operating_body = "نربط الهوية العلمية بالأدلة الموثقة والتوفر الفعلي، مع إبقاء نتائج البحث منفصلة عن ادعاءات البائعين.";
            t.about_path_note = "ينبغي أن يكون كل ادعاء علمي قابلاً للتتبع إلى مصدر، وأن تزداد موثوقية كل سجل مع تحسن الأدلة والمراجعة.";
            t.footer_contact = "اتصل بنا";
            t.footer_legal = "قانوني";
            t.footer_mission = "المهمة";
            t.footer_contact_us = "اتصل بنا";
            t.footer_support = "الدعم";
            t.footer_work_with_us = "اعمل معنا";
            t.footer_account = "الحساب";
            t.footer_legal_link = "الشؤون القانونية";
            t.footer_privacy_policy = "سياسة الخصوصية";
            t.footer_terms_of_service = "شروط الخدمة";
            t.footer_returns_and_refunds = "الإرجاع والاسترداد";
            t.footer_shipping = "الشحن";
            t.footer_about_us = "من نحن";
            t.footer_conflict_free_minerals = "معادن خالية من النزاعات";
            t.footer_faq = "الأسئلة الشائعة";
            t.footer_powered_trust_by = "بدعم موثوق من";
            t.report_title_suffix = "تقرير المعدن";
            t.context_heading = "السياق";
            t.snapshot_heading = "الملخص الفيزيائي والكيميائي";
            t.summary_heading = "الملخص التفسيري";
            t.major_elements_heading = "العناصر الرئيسية";
            t.notes_heading = "ملاحظات";
        }
        Language::Fr => {
            t.registry = registry_fr();
            t.review = review_fr();
            t.nav_home = "Accueil";
            t.nav_all_minerals = "Tous les minéraux";
            t.nav_about = "À propos";
            t.nav_login = "connexion";
            t.session_public_mode = "Mode public";
            t.home_title = "Minéraux";
            t.home_subtitle = "Choisissez la langue puis ouvrez le catalogue.";
            t.home_select_language = "Langue";
            t.home_continue = "Continuer";
            t.catalog_title = "Catalogue des minéraux";
            t.catalog_subtitle = "Explorez des fiches de minéraux présentant clairement leurs données physiques, chimiques et de provenance.";
            t.no_minerals = "Aucune fiche de minéral n’est encore disponible.";
            t.open_mineral = "Ouvrir le minéral";
            t.all_minerals_title = "Les minéraux du monde";
            t.all_minerals_subtitle = "Découvrez les espèces minérales du monde entier et suivez notre catalogue croissant de fiches documentées.";
            t.all_minerals_published_label = "Fiches disponibles maintenant";
            t.all_minerals_estimated_label = "Espèces minérales connues dans le monde";
            t.all_minerals_disclaimer = "La couverture progresse avec l’ajout de sources fiables et de relectures. Les informations non confirmées restent clairement signalées.";
            t.label_family = "Famille";
            t.label_description = "Description";
            t.label_crystal_system = "Système cristallin";
            t.label_notes = "Notes";
            t.mineral_profile = "Profil du minéral";
            t.major_composition = "Composition chimique principale";
            t.computed_classification = "Classification calculée";
            t.report_builder = "Générateur de rapport";
            t.generate_pdf = "Générer le PDF";
            t.status_pdf_failed = "Échec de génération du PDF.";
            t.current_chain_output = "Synthèse de la recherche";
            t.recommendations_heading = "Recommandations";
            t.about_title = "À propos de Minerals";
            t.about_subtitle = "Une plateforme ouverte de connaissances et d’approvisionnement pour des informations fiables sur les minéraux.";
            t.about_operating_model = "Notre approche";
            t.about_operating_body = "Nous relions l’identité scientifique, les sources citées et la disponibilité réelle, tout en séparant les résultats de recherche des affirmations commerciales.";
            t.about_path_note = "Chaque affirmation scientifique doit être rattachée à une source, et chaque fiche doit gagner en fiabilité à mesure que les preuves et la relecture progressent.";
            t.footer_contact = "Contact";
            t.footer_legal = "Mentions légales";
            t.footer_mission = "Mission";
            t.footer_contact_us = "contactez-nous";
            t.footer_support = "support";
            t.footer_work_with_us = "travaillez avec nous";
            t.footer_account = "compte";
            t.footer_legal_link = "mentions légales";
            t.footer_privacy_policy = "politique de confidentialité";
            t.footer_terms_of_service = "conditions d'utilisation";
            t.footer_returns_and_refunds = "retours et remboursements";
            t.footer_shipping = "livraison";
            t.footer_about_us = "à propos de nous";
            t.footer_conflict_free_minerals = "minéraux sans conflit";
            t.footer_faq = "questions fréquentes";
            t.footer_powered_trust_by = "propulsé par";
            t.report_title_suffix = "Rapport minéral";
            t.context_heading = "Contexte";
            t.snapshot_heading = "Aperçu physique et chimique";
            t.summary_heading = "Résumé interprétatif";
            t.major_elements_heading = "Éléments majeurs";
        }
        Language::De => {
            t.registry = registry_de();
            t.review = review_de();
            t.nav_home = "Start";
            t.nav_all_minerals = "Alle Minerale";
            t.nav_about = "Über uns";
            t.nav_login = "anmelden";
            t.home_title = "Minerale";
            t.home_subtitle = "Sprache wählen und zum Mineralkatalog wechseln.";
            t.home_select_language = "Sprache";
            t.home_continue = "Weiter";
            t.catalog_title = "Mineralkatalog";
            t.catalog_subtitle = "Entdecken Sie Mineralprofile mit klaren Angaben zu physikalischen, chemischen und herkunftsbezogenen Eigenschaften.";
            t.no_minerals = "Derzeit sind noch keine Mineralprofile verfügbar.";
            t.open_mineral = "Mineral öffnen";
            t.all_minerals_title = "Mineralien der Welt";
            t.all_minerals_subtitle = "Entdecken Sie Mineralarten aus aller Welt und verfolgen Sie unseren wachsenden Katalog recherchierter Profile.";
            t.all_minerals_published_label = "Derzeit verfügbare Profile";
            t.all_minerals_estimated_label = "Weltweit bekannte Mineralarten";
            t.all_minerals_disclaimer = "Die Abdeckung wächst mit verlässlichen Quellen und weiterer Prüfung. Unbestätigte Angaben bleiben deutlich gekennzeichnet.";
            t.label_family = "Familie";
            t.label_description = "Beschreibung";
            t.label_crystal_system = "Kristallsystem";
            t.label_notes = "Notizen";
            t.mineral_profile = "Mineralprofil";
            t.report_builder = "Berichtsgenerator";
            t.generate_pdf = "PDF erzeugen";
            t.status_pdf_failed = "PDF-Erzeugung fehlgeschlagen.";
            t.current_chain_output = "Forschungszusammenfassung";
            t.recommendations_heading = "Empfehlungen";
            t.about_title = "Über Minerals";
            t.about_subtitle = "Eine offene Wissens- und Beschaffungsplattform für verlässliche Informationen über Mineralien.";
            t.about_operating_model = "Unser Ansatz";
            t.about_operating_body = "Wir verbinden wissenschaftliche Identität, zitierte Quellen und tatsächliche Verfügbarkeit und trennen Forschungsergebnisse von Händlerangaben.";
            t.about_path_note = "Jede wissenschaftliche Aussage soll zu einer Quelle zurückverfolgbar sein, und jeder Eintrag soll durch bessere Belege und Prüfung verlässlicher werden.";
            t.footer_contact = "Kontakt";
            t.footer_legal = "Rechtliches";
            t.footer_mission = "Mission";
            t.footer_contact_us = "kontakt";
            t.footer_support = "support";
            t.footer_work_with_us = "arbeite mit uns";
            t.footer_account = "konto";
            t.footer_legal_link = "rechtliches";
            t.footer_privacy_policy = "datenschutz";
            t.footer_terms_of_service = "nutzungsbedingungen";
            t.footer_returns_and_refunds = "rückgabe und erstattung";
            t.footer_shipping = "versand";
            t.footer_about_us = "über uns";
            t.footer_conflict_free_minerals = "konfliktfreie mineralien";
            t.footer_faq = "häufige fragen";
            t.footer_powered_trust_by = "bereitgestellt von";
            t.report_title_suffix = "Mineralbericht";
            t.context_heading = "Kontext";
            t.snapshot_heading = "Physikalisch-chemische Übersicht";
            t.summary_heading = "Interpretative Zusammenfassung";
            t.major_elements_heading = "Hauptelemente";
        }
        Language::Pt => {
            t.registry = registry_pt();
            t.review = review_pt();
            t.nav_home = "Início";
            t.nav_all_minerals = "Todos os minerais";
            t.nav_about = "Sobre";
            t.nav_login = "entrar";
            t.home_title = "Minerais";
            t.home_subtitle = "Selecione o idioma e continue para o catálogo.";
            t.home_select_language = "Idioma";
            t.home_continue = "Continuar";
            t.catalog_title = "Catálogo de minerais";
            t.catalog_subtitle = "Explore perfis de minerais com informações físicas, químicas e de procedência apresentadas com clareza.";
            t.no_minerals = "Ainda não há perfis de minerais disponíveis.";
            t.open_mineral = "Abrir mineral";
            t.all_minerals_title = "Minerais do mundo";
            t.all_minerals_subtitle = "Descubra espécies minerais de todo o mundo e acompanhe nosso catálogo crescente de perfis pesquisados.";
            t.all_minerals_published_label = "Perfis disponíveis agora";
            t.all_minerals_estimated_label = "Espécies minerais conhecidas no mundo";
            t.all_minerals_disclaimer = "A cobertura cresce com a inclusão de fontes confiáveis e revisão. Informações não confirmadas permanecem claramente sinalizadas.";
            t.label_family = "Família";
            t.label_description = "Descrição";
            t.label_crystal_system = "Sistema cristalino";
            t.label_notes = "Notas";
            t.mineral_profile = "Perfil do mineral";
            t.report_builder = "Gerador de relatório";
            t.generate_pdf = "Gerar PDF";
            t.status_pdf_failed = "Falha ao gerar PDF.";
            t.current_chain_output = "Resumo da pesquisa";
            t.recommendations_heading = "Recomendações";
            t.about_title = "Sobre o Minerals";
            t.about_subtitle = "Uma plataforma aberta de conhecimento e fornecimento para informações confiáveis sobre minerais.";
            t.about_operating_model = "Nossa abordagem";
            t.about_operating_body = "Conectamos identidade científica, evidências citadas e disponibilidade real, mantendo resultados de pesquisa separados de alegações comerciais.";
            t.about_path_note = "Toda afirmação científica deve ser rastreável até uma fonte, e cada registro deve se tornar mais confiável conforme melhoram as evidências e a revisão.";
            t.footer_contact = "Contato";
            t.footer_legal = "Jurídico";
            t.footer_mission = "Missão";
            t.footer_contact_us = "fale conosco";
            t.footer_support = "suporte";
            t.footer_work_with_us = "trabalhe conosco";
            t.footer_account = "conta";
            t.footer_legal_link = "jurídico";
            t.footer_privacy_policy = "política de privacidade";
            t.footer_terms_of_service = "termos de serviço";
            t.footer_returns_and_refunds = "devoluções e reembolsos";
            t.footer_shipping = "envio";
            t.footer_about_us = "sobre nós";
            t.footer_conflict_free_minerals = "minerais livres de conflito";
            t.footer_faq = "perguntas frequentes";
            t.footer_powered_trust_by = "com confiança por";
            t.report_title_suffix = "Relatório mineral";
            t.context_heading = "Contexto";
            t.snapshot_heading = "Resumo físico e químico";
            t.summary_heading = "Resumo interpretativo";
            t.major_elements_heading = "Elementos principais";
        }
        Language::Hi => {
            t.registry = registry_hi();
            t.review = review_hi();
            t.nav_home = "होम";
            t.nav_all_minerals = "सभी खनिज";
            t.nav_about = "परिचय";
            t.nav_login = "लॉगिन";
            t.home_title = "मिनरल्स";
            t.home_subtitle = "भाषा चुनें और खनिज कैटलॉग में जाएँ।";
            t.home_select_language = "भाषा";
            t.home_continue = "आगे बढ़ें";
            t.catalog_title = "खनिज कैटलॉग";
            t.catalog_subtitle = "स्पष्ट भौतिक, रासायनिक और स्रोत संबंधी जानकारी वाले खनिज प्रोफ़ाइल देखें।";
            t.no_minerals = "अभी कोई खनिज प्रोफ़ाइल उपलब्ध नहीं है।";
            t.open_mineral = "खनिज खोलें";
            t.all_minerals_title = "विश्व के खनिज";
            t.all_minerals_subtitle =
                "दुनिया भर की खनिज प्रजातियाँ खोजें और शोधित प्रोफ़ाइल के हमारे बढ़ते कैटलॉग को देखें।";
            t.all_minerals_published_label = "अभी उपलब्ध प्रोफ़ाइल";
            t.all_minerals_estimated_label = "विश्व भर में ज्ञात खनिज प्रजातियाँ";
            t.all_minerals_disclaimer = "विश्वसनीय स्रोत और समीक्षा जुड़ने के साथ कवरेज बढ़ता है। अपुष्ट जानकारी को स्पष्ट रूप से चिह्नित रखा जाता है।";
            t.label_family = "परिवार";
            t.label_description = "विवरण";
            t.label_notes = "टिप्पणियाँ";
            t.mineral_profile = "खनिज प्रोफ़ाइल";
            t.major_composition = "मुख्य रासायनिक संरचना";
            t.computed_classification = "गणना-आधारित वर्गीकरण";
            t.report_builder = "रिपोर्ट बिल्डर";
            t.generate_pdf = "PDF बनाएँ";
            t.status_pdf_failed = "PDF निर्माण विफल हुआ।";
            t.current_chain_output = "शोध सारांश";
            t.recommendations_heading = "सिफारिशें";
            t.about_title = "Minerals के बारे में";
            t.about_subtitle = "खनिजों की भरोसेमंद जानकारी के लिए एक खुला ज्ञान और आपूर्ति मंच।";
            t.about_operating_model = "हमारा दृष्टिकोण";
            t.about_operating_body = "हम वैज्ञानिक पहचान, उद्धृत प्रमाण और वास्तविक उपलब्धता को जोड़ते हैं, जबकि शोध निष्कर्षों को विक्रेता दावों से अलग रखते हैं।";
            t.about_path_note = "हर वैज्ञानिक दावे का स्रोत पता लगना चाहिए, और बेहतर प्रमाण व समीक्षा के साथ हर रिकॉर्ड अधिक विश्वसनीय बनना चाहिए।";
            t.footer_contact = "संपर्क";
            t.footer_legal = "कानूनी";
            t.footer_mission = "मिशन";
            t.footer_contact_us = "हमसे संपर्क करें";
            t.footer_support = "सहायता";
            t.footer_work_with_us = "हमारे साथ काम करें";
            t.footer_account = "खाता";
            t.footer_legal_link = "कानूनी";
            t.footer_privacy_policy = "गोपनीयता नीति";
            t.footer_terms_of_service = "सेवा की शर्तें";
            t.footer_returns_and_refunds = "रिटर्न और रिफंड";
            t.footer_shipping = "शिपिंग";
            t.footer_about_us = "हमारे बारे में";
            t.footer_conflict_free_minerals = "संघर्ष-मुक्त खनिज";
            t.footer_faq = "अक्सर पूछे जाने वाले प्रश्न";
            t.footer_powered_trust_by = "विश्वसनीय साझेदार";
            t.report_title_suffix = "खनिज रिपोर्ट";
            t.context_heading = "संदर्भ";
            t.snapshot_heading = "भौतिक और रासायनिक सारांश";
            t.summary_heading = "व्याख्यात्मक सार";
            t.major_elements_heading = "मुख्य तत्व";
        }
        Language::Ja => {
            t.registry = registry_ja();
            t.review = review_ja();
            t.nav_home = "ホーム";
            t.nav_all_minerals = "全鉱物";
            t.nav_about = "概要";
            t.nav_login = "ログイン";
            t.home_title = "ミネラル";
            t.home_subtitle = "言語を選択して鉱物カタログへ進みます。";
            t.home_select_language = "言語";
            t.home_continue = "続行";
            t.catalog_title = "鉱物カタログ";
            t.catalog_subtitle =
                "物理・化学的性質と由来を分かりやすく示した鉱物プロフィールを閲覧できます。";
            t.no_minerals = "現在、利用できる鉱物プロフィールはありません。";
            t.open_mineral = "鉱物を開く";
            t.all_minerals_title = "世界の鉱物";
            t.all_minerals_subtitle =
                "世界各地の鉱物種を発見し、調査済みプロフィールの拡充をご覧ください。";
            t.all_minerals_published_label = "現在利用できるプロフィール";
            t.all_minerals_estimated_label = "世界で知られている鉱物種";
            t.all_minerals_disclaimer = "信頼できる資料と確認が加わるにつれて収録範囲を広げます。未確認の情報は明確に表示します。";
            t.label_family = "分類";
            t.label_description = "説明";
            t.label_crystal_system = "結晶系";
            t.label_notes = "ノート";
            t.mineral_profile = "鉱物プロフィール";
            t.major_composition = "主要化学組成";
            t.computed_classification = "計算分類";
            t.report_builder = "レポート生成";
            t.generate_pdf = "PDFを生成";
            t.status_pdf_failed = "PDF 生成に失敗しました。";
            t.current_chain_output = "調査概要";
            t.recommendations_heading = "推奨事項";
            t.about_title = "Minerals について";
            t.about_subtitle =
                "鉱物の信頼できる情報を提供する、オープンな知識・調達プラットフォームです。";
            t.about_operating_model = "私たちの方針";
            t.about_operating_body = "科学的な同定、引用資料、実際の入手可能性を結び付け、研究結果と販売者の情報を分けて扱います。";
            t.about_path_note = "すべての科学的主張は出典までたどれるようにし、資料と確認が充実するほど各記録の信頼性が高まることを目指します。";
            t.footer_contact = "お問い合わせ";
            t.footer_legal = "法務";
            t.footer_mission = "ミッション";
            t.footer_contact_us = "お問い合わせ";
            t.footer_support = "サポート";
            t.footer_work_with_us = "採用情報";
            t.footer_account = "アカウント";
            t.footer_legal_link = "法務情報";
            t.footer_privacy_policy = "プライバシーポリシー";
            t.footer_terms_of_service = "利用規約";
            t.footer_returns_and_refunds = "返品・返金";
            t.footer_shipping = "配送";
            t.footer_about_us = "私たちについて";
            t.footer_conflict_free_minerals = "紛争鉱物フリー";
            t.footer_faq = "よくある質問";
            t.footer_powered_trust_by = "提供";
            t.report_title_suffix = "鉱物レポート";
            t.context_heading = "コンテキスト";
            t.snapshot_heading = "物理・化学スナップショット";
            t.summary_heading = "解釈サマリー";
            t.major_elements_heading = "主要元素";
        }
    }

    t
}

#[cfg(test)]
mod tests {
    use super::{
        ingestion_text, material_fact_label, ui_text, IngestionText, Language, RegistryText,
    };

    const PUBLIC_FACT_KEYS: &[&str] = &[
        "appearance",
        "boiling_point_c",
        "color",
        "colour",
        "crystal_system",
        "density_g_cm3",
        "disposal",
        "first_aid",
        "handling",
        "hardness_mohs",
        "hazards",
        "luster",
        "lustre",
        "major_elements_pct",
        "melting_point_c",
        "molar_mass_g_mol",
        "notes",
        "ppe",
        "storage",
        "streak",
    ];

    fn ingestion_fields(text: IngestionText) -> Vec<(&'static str, &'static str)> {
        vec![
            ("link", text.link),
            ("title", text.title),
            ("subtitle", text.subtitle),
            ("back_to_admin", text.back_to_admin),
            ("individual_review_link", text.individual_review_link),
            ("create_title", text.create_title),
            ("create_hint", text.create_hint),
            ("manifest_payload", text.manifest_payload),
            ("dataset", text.dataset),
            ("source_name", text.source_name),
            ("source_url", text.source_url),
            ("attribution_review", text.attribution_review),
            (
                "historical_attribution_missing",
                text.historical_attribution_missing,
            ),
            ("release_version", text.release_version),
            ("released_at", text.released_at),
            ("retrieved_at", text.retrieved_at),
            ("manifest_hash", text.manifest_hash),
            ("artifact_hash", text.artifact_hash),
            ("parser_name", text.parser_name),
            ("parser_version", text.parser_version),
            ("parser_revision", text.parser_revision),
            ("parser_configuration_hash", text.parser_configuration_hash),
            ("record_license", text.record_license),
            ("expected_chunks", text.expected_chunks),
            ("expected_records", text.expected_records),
            ("create_batch", text.create_batch),
            ("upload_title", text.upload_title),
            ("upload_hint", text.upload_hint),
            ("batch_id", text.batch_id),
            ("chunk_number", text.chunk_number),
            ("chunk_payload", text.chunk_payload),
            ("upload_chunk", text.upload_chunk),
            ("finalize_title", text.finalize_title),
            ("finalize_hint", text.finalize_hint),
            ("finalize_validate", text.finalize_validate),
            ("batches_title", text.batches_title),
            ("empty", text.empty),
            ("status", text.status),
            ("status_receiving", text.status_receiving),
            ("status_ready", text.status_ready),
            ("status_needs_attention", text.status_needs_attention),
            ("status_approved", text.status_approved),
            ("status_rejected", text.status_rejected),
            ("progress", text.progress),
            ("created_at", text.created_at),
            ("finalized_at", text.finalized_at),
            ("uploaded_chunks", text.uploaded_chunks),
            ("records", text.records),
            ("created", text.created),
            ("adopted", text.adopted),
            ("updated", text.updated),
            ("unchanged", text.unchanged),
            ("missing", text.missing),
            ("blockers", text.blockers),
            ("identity_warnings", text.identity_warnings),
            ("anomaly_samples", text.anomaly_samples),
            ("no_anomalies", text.no_anomalies),
            ("review_samples", text.review_samples),
            ("source_record", text.source_record),
            ("nomenclature_status", text.nomenclature_status),
            ("valid_species", text.valid_species),
            ("yes", text.yes),
            ("no", text.no),
            ("report_hash", text.report_hash),
            ("base_batch", text.base_batch),
            ("decision_title", text.decision_title),
            ("decision_warning", text.decision_warning),
            ("acknowledge_warning", text.acknowledge_warning),
            ("confirmation", text.confirmation),
            ("confirmation_hint", text.confirmation_hint),
            ("operator_note", text.operator_note),
            ("operator_note_placeholder", text.operator_note_placeholder),
            ("approve_release", text.approve_release),
            ("reject_release", text.reject_release),
            ("working", text.working),
            ("request_failed", text.request_failed),
            ("batch_created", text.batch_created),
            ("chunk_uploaded", text.chunk_uploaded),
            ("validation_complete", text.validation_complete),
            ("approved_notice", text.approved_notice),
            ("rejected_notice", text.rejected_notice),
        ]
    }

    fn mineral_detail_fields(text: RegistryText) -> Vec<(&'static str, &'static str)> {
        vec![
            ("ima_number", text.ima_number),
            ("ima_symbol", text.ima_symbol),
            ("mineral_species", text.mineral_species),
            ("nomenclature_ima_approved", text.nomenclature_ima_approved),
            ("nomenclature_recognized", text.nomenclature_recognized),
            ("nomenclature_redefined", text.nomenclature_redefined),
            ("nomenclature_renamed", text.nomenclature_renamed),
            ("nomenclature_uncertain", text.nomenclature_uncertain),
            ("nomenclature_questionable", text.nomenclature_questionable),
            ("nomenclature_discredited", text.nomenclature_discredited),
            ("nomenclature_unknown", text.nomenclature_unknown),
            ("valid_mineral_species", text.valid_mineral_species),
            ("mineral_facts", text.mineral_facts),
            ("nomenclature", text.nomenclature),
            ("source_status_approved", text.source_status_approved),
            (
                "source_status_grandfathered",
                text.source_status_grandfathered,
            ),
            ("source_status_redefined", text.source_status_redefined),
            ("source_status_renamed", text.source_status_renamed),
            ("source_status_uncertain", text.source_status_uncertain),
            (
                "source_status_questionable",
                text.source_status_questionable,
            ),
            ("source_status_discredited", text.source_status_discredited),
            ("source_status_unknown", text.source_status_unknown),
            ("discovery_country", text.discovery_country),
            (
                "official_identity_coverage_note",
                text.official_identity_coverage_note,
            ),
            ("published_references", text.published_references),
            ("source_and_license", text.source_and_license),
        ]
    }

    #[test]
    fn mineral_detail_copy_is_complete_for_every_language() {
        for &language in Language::all() {
            for (field, value) in mineral_detail_fields(ui_text(language).registry) {
                assert!(
                    !value.trim().is_empty(),
                    "empty mineral detail {field} for {}",
                    language.code()
                );
            }
        }
    }

    #[test]
    fn ordinary_mineral_detail_copy_is_localized_outside_english() {
        let english = ui_text(Language::En).registry;

        for &language in Language::all() {
            if language == Language::En {
                continue;
            }
            let localized = ui_text(language).registry;
            for (field, value, english_value) in [
                ("ima_number", localized.ima_number, english.ima_number),
                (
                    "mineral_species",
                    localized.mineral_species,
                    english.mineral_species,
                ),
                (
                    "valid_mineral_species",
                    localized.valid_mineral_species,
                    english.valid_mineral_species,
                ),
                (
                    "mineral_facts",
                    localized.mineral_facts,
                    english.mineral_facts,
                ),
                (
                    "source_status_grandfathered",
                    localized.source_status_grandfathered,
                    english.source_status_grandfathered,
                ),
                (
                    "discovery_country",
                    localized.discovery_country,
                    english.discovery_country,
                ),
                (
                    "official_identity_coverage_note",
                    localized.official_identity_coverage_note,
                    english.official_identity_coverage_note,
                ),
                (
                    "published_references",
                    localized.published_references,
                    english.published_references,
                ),
                (
                    "source_and_license",
                    localized.source_and_license,
                    english.source_and_license,
                ),
                ("attribution", localized.attribution, english.attribution),
                (
                    "attribution_party",
                    localized.attribution_party,
                    english.attribution_party,
                ),
                ("source_work", localized.source_work, english.source_work),
                (
                    "license_terms",
                    localized.license_terms,
                    english.license_terms,
                ),
                ("changes_made", localized.changes_made, english.changes_made),
                (
                    "no_endorsement",
                    localized.no_endorsement,
                    english.no_endorsement,
                ),
                (
                    "derived_data_license",
                    localized.derived_data_license,
                    english.derived_data_license,
                ),
            ] {
                assert_ne!(
                    value,
                    english_value,
                    "mineral detail {field} remained English for {}",
                    language.code()
                );
            }
        }
    }

    #[test]
    fn mineral_detail_template_does_not_embed_targeted_english_copy_or_raw_statuses() {
        let template = include_str!("../static/material_record.html");
        for forbidden in [
            ">Mineral species<",
            "IMA approved\n",
            "Recognized (grandfathered)",
            "Approved by the IMA",
            ">Mineral facts<",
            ">Nomenclature<",
            ">Discovery country<",
            "This entry currently documents the mineral's official identity",
            ">Published references<",
            ">Source and license<",
            "{{ material.nomenclature_status }}",
        ] {
            assert!(
                !template.contains(forbidden),
                "mineral detail template still embeds {forbidden:?}"
            );
        }

        for required in [
            "txt.registry.ima_number",
            "txt.registry.ima_symbol",
            "txt.registry.mineral_species",
            "txt.registry.source_status_approved",
            "txt.registry.discovery_country",
            "txt.registry.official_identity_coverage_note",
            "txt.registry.published_references",
            "txt.registry.source_and_license",
        ] {
            assert!(
                template.contains(required),
                "mineral detail template does not use {required}"
            );
        }
    }

    #[test]
    fn ingestion_copy_is_complete_and_wired_for_every_language() {
        for &language in Language::all() {
            let actual = ui_text(language).ingestion;
            let expected = ingestion_text(language);
            assert_eq!(
                ingestion_fields(actual),
                ingestion_fields(expected),
                "ui_text did not select ingestion copy for {}",
                language.code()
            );

            for (field, value) in ingestion_fields(actual) {
                assert!(
                    !value.trim().is_empty(),
                    "empty ingestion {field} for {}",
                    language.code()
                );
            }
        }
    }

    #[test]
    fn ordinary_ingestion_copy_is_localized_outside_english() {
        let english = ui_text(Language::En).ingestion;

        for &language in Language::all() {
            if language == Language::En {
                continue;
            }

            let localized = ui_text(language).ingestion;
            for (field, value, english_value) in [
                ("link", localized.link, english.link),
                ("title", localized.title, english.title),
                ("subtitle", localized.subtitle, english.subtitle),
                (
                    "batches_title",
                    localized.batches_title,
                    english.batches_title,
                ),
                (
                    "back_to_admin",
                    localized.back_to_admin,
                    english.back_to_admin,
                ),
                (
                    "individual_review_link",
                    localized.individual_review_link,
                    english.individual_review_link,
                ),
                ("create_hint", localized.create_hint, english.create_hint),
                ("dataset", localized.dataset, english.dataset),
                ("released_at", localized.released_at, english.released_at),
                ("retrieved_at", localized.retrieved_at, english.retrieved_at),
                (
                    "artifact_hash",
                    localized.artifact_hash,
                    english.artifact_hash,
                ),
                (
                    "parser_revision",
                    localized.parser_revision,
                    english.parser_revision,
                ),
                (
                    "parser_configuration_hash",
                    localized.parser_configuration_hash,
                    english.parser_configuration_hash,
                ),
                (
                    "expected_records",
                    localized.expected_records,
                    english.expected_records,
                ),
                ("empty", localized.empty, english.empty),
                (
                    "identity_warnings",
                    localized.identity_warnings,
                    english.identity_warnings,
                ),
                (
                    "review_samples",
                    localized.review_samples,
                    english.review_samples,
                ),
                (
                    "source_record",
                    localized.source_record,
                    english.source_record,
                ),
                (
                    "nomenclature_status",
                    localized.nomenclature_status,
                    english.nomenclature_status,
                ),
                (
                    "valid_species",
                    localized.valid_species,
                    english.valid_species,
                ),
                ("yes", localized.yes, english.yes),
                ("created_at", localized.created_at, english.created_at),
                ("finalized_at", localized.finalized_at, english.finalized_at),
                (
                    "status_receiving",
                    localized.status_receiving,
                    english.status_receiving,
                ),
                ("status_ready", localized.status_ready, english.status_ready),
                (
                    "status_needs_attention",
                    localized.status_needs_attention,
                    english.status_needs_attention,
                ),
                (
                    "status_approved",
                    localized.status_approved,
                    english.status_approved,
                ),
                (
                    "status_rejected",
                    localized.status_rejected,
                    english.status_rejected,
                ),
                (
                    "decision_title",
                    localized.decision_title,
                    english.decision_title,
                ),
                ("working", localized.working, english.working),
                (
                    "request_failed",
                    localized.request_failed,
                    english.request_failed,
                ),
            ] {
                assert_ne!(
                    value,
                    english_value,
                    "ingestion {field} remained English for {}",
                    language.code()
                );
            }

            if language == Language::Es {
                assert_eq!(localized.no, "No");
            } else {
                assert_ne!(
                    localized.no,
                    english.no,
                    "ingestion no remained English for {}",
                    language.code()
                );
            }
        }
    }

    #[test]
    fn visible_ingestion_copy_describes_the_database_not_the_import_pipeline() {
        let english = ui_text(Language::En).ingestion;
        assert_eq!(english.link, "Mineral database");
        assert_eq!(english.title, "Mineral database");
        assert_eq!(
            english.subtitle,
            "Official mineral releases and their publication status."
        );

        for &language in Language::all() {
            let text = ui_text(language).ingestion;
            let visible_copy = [
                text.link,
                text.title,
                text.subtitle,
                text.batches_title,
                text.empty,
                text.attribution_review,
                text.historical_attribution_missing,
            ]
            .join(" ")
            .to_lowercase();
            let forbidden_terms: &[&str] = match language {
                Language::En => &["bulk", "ingestion", "create", "upload", "chunk", "manifest"],
                Language::Es => &["ingesta", "crear", "carga", "fragment", "manifiest", "lote"],
                Language::Cs => &["import", "vytvář", "nahrá", "část", "manifest", "dávk"],
                Language::De => &["import", "erstell", "hochlad", "teil", "manifest", "stapel"],
                Language::Fr => &["import", "crée", "télévers", "fragment", "manifeste", "lot"],
                Language::Pt => &["ingest", "crie", "envie", "parte", "manifest", "lote"],
                Language::Zh => &["导入", "创建", "上传", "分块", "清单", "批次"],
                Language::Ja => &[
                    "取り込み",
                    "作成",
                    "アップロード",
                    "チャンク",
                    "マニフェスト",
                    "バッチ",
                ],
                Language::Ar => &["استيراد", "أنشئ", "تحميل", "جزء", "البيان", "دفعة"],
                Language::Hi => &["आयात", "बनाएँ", "अपलोड", "चंक", "मैनिफ़ेस्ट", "बैच"],
            };

            for forbidden in forbidden_terms {
                assert!(
                    !visible_copy.contains(forbidden),
                    "visible ingestion copy contains pipeline term {forbidden:?} for {}",
                    language.code()
                );
            }
        }
    }

    #[test]
    fn ingestion_confirmation_targets_the_exact_release_version() {
        for &language in Language::all() {
            let text = ui_text(language).ingestion;
            let hint = text.confirmation_hint.to_lowercase();
            let release_version = text.release_version.to_lowercase();
            let batch_id = text.batch_id.to_lowercase();
            let exactness_marker = match language {
                Language::En => "exactly",
                Language::Es => "exactamente",
                Language::Cs => "přesné",
                Language::De => "exakt",
                Language::Fr => "exactement",
                Language::Pt => "exatamente",
                Language::Zh => "完全一致",
                Language::Ja => "正確",
                Language::Ar => "تمامًا",
                Language::Hi => "ठीक उसी तरह",
            };

            assert!(
                hint.contains(release_version.as_str()),
                "confirmation does not name release_version for {}",
                language.code()
            );
            assert!(
                hint.contains(exactness_marker),
                "confirmation does not require an exact value for {}",
                language.code()
            );
            assert!(
                !hint.contains(batch_id.as_str()),
                "confirmation incorrectly asks for batch_id for {}",
                language.code()
            );
        }
    }

    #[test]
    fn discovery_copy_is_mineral_only_in_every_language() {
        let english_about_subtitle = ui_text(Language::En).about_subtitle;

        for &language in Language::all() {
            let text = ui_text(language);
            let copy = [
                text.registry.eyebrow,
                text.registry.title,
                text.registry.subtitle,
                text.registry.search_placeholder,
                text.registry.empty_results,
                text.registry.detail_suffix,
                text.registry.no_offers_body,
                text.about_subtitle,
            ]
            .join(" ")
            .to_lowercase();
            let (mineral_word, forbidden_words): (&str, &[&str]) = match language {
                Language::En => ("mineral", &["compound", "material"]),
                Language::Es => ("mineral", &["compuesto", "material"]),
                Language::Cs => ("minerál", &["sloučen", "materiál"]),
                Language::De => ("mineral", &["verbindung", "material"]),
                Language::Fr => ("minéral", &["composé", "matériau"]),
                Language::Pt => ("mineral", &["composto", "material"]),
                Language::Zh => ("矿物", &["化合物", "材料"]),
                Language::Ar => ("المعادن", &["المركبات", "المواد", "المادة"]),
                Language::Hi => ("खनिज", &["यौगिक", "सामग्री"]),
                Language::Ja => ("鉱物", &["化合物", "材料"]),
            };

            assert!(
                copy.contains(mineral_word),
                "mineral discovery is unclear for {}",
                language.code()
            );
            for forbidden in forbidden_words {
                assert!(
                    !copy.contains(forbidden),
                    "mineral-only copy for {} contains {forbidden}",
                    language.code()
                );
            }
            assert!(!copy.contains("nacl"));
            assert!(!copy.contains("7647-14-5"));
            assert!(!text.nav_all_minerals.trim().is_empty());
            assert!(!text.about_subtitle.trim().is_empty());
            if language != Language::En {
                assert_ne!(
                    text.about_subtitle,
                    english_about_subtitle,
                    "about subtitle fell back to English for {}",
                    language.code()
                );
            }
        }
    }

    #[test]
    fn every_public_fact_has_a_label_in_every_language() {
        for &language in Language::all() {
            for &key in PUBLIC_FACT_KEYS {
                let label = material_fact_label(language, key)
                    .unwrap_or_else(|| panic!("missing {key} label for {}", language.code()));
                assert!(
                    !label.trim().is_empty(),
                    "empty {key} label for {}",
                    language.code()
                );
            }
        }
    }

    #[test]
    fn ordinary_labels_are_localized_outside_english() {
        for &language in Language::all() {
            if language == Language::En {
                continue;
            }
            for key in ["appearance", "hazards", "storage", "streak"] {
                assert_ne!(
                    material_fact_label(language, key),
                    material_fact_label(Language::En, key),
                    "{key} remained English for {}",
                    language.code()
                );
            }
        }
    }

    #[test]
    fn review_queue_and_claim_labels_are_localized() {
        let english = ui_text(Language::En);
        assert!(english
            .review
            .decision_warning
            .contains("does not upgrade its scientific verification status"));

        for &language in Language::all() {
            let text = ui_text(language);
            for label in [
                text.review.queue_link,
                text.review.title,
                text.review.subtitle,
                text.review.operator_note,
                text.review.approve,
                text.review.reject,
                text.review.approved_notice,
                text.review.rejected_notice,
                text.review.creates_new,
                text.review.updates_existing,
                text.review.view_current,
                text.review.review_id,
                text.review.slug,
                text.review.cas_number,
                text.review.synonyms,
                text.review.record_license,
                text.review.claim_value,
                text.review.claim_locator,
                text.review.claim_note,
                text.review.claim_details,
                text.review.retrieved_at,
                text.review.content_hash,
                text.review.complete_payload,
                text.review.complete_payload_hint,
                text.review.decision_warning,
                text.registry.supports,
                text.registry.source_license,
                text.registry.attribution,
                text.registry.attribution_party,
                text.registry.source_work,
                text.registry.license_terms,
                text.registry.changes_made,
                text.registry.no_endorsement,
                text.registry.derived_data_license,
            ] {
                assert!(
                    !label.trim().is_empty(),
                    "empty review or claim label for {}",
                    language.code()
                );
            }

            if language != Language::En {
                assert_ne!(text.review.queue_link, english.review.queue_link);
                assert_ne!(text.review.approve, english.review.approve);
                assert_ne!(text.review.reject, english.review.reject);
                assert_ne!(text.review.approved_notice, english.review.approved_notice);
                assert_ne!(text.review.rejected_notice, english.review.rejected_notice);
                assert_ne!(text.registry.supports, english.registry.supports);
                assert_ne!(
                    text.registry.source_license,
                    english.registry.source_license
                );
            }
        }
    }

    #[test]
    fn review_decision_notices_describe_completed_localized_actions() {
        for &language in Language::all() {
            let text = ui_text(language).review;
            let (approved, rejected) = match language {
                Language::En => (
                    "Mineral revision approved and published.",
                    "Mineral revision rejected.",
                ),
                Language::Es => (
                    "La revisión del mineral fue aprobada y publicada.",
                    "La revisión del mineral fue rechazada.",
                ),
                Language::Cs => (
                    "Revize minerálu byla schválena a zveřejněna.",
                    "Revize minerálu byla zamítnuta.",
                ),
                Language::De => (
                    "Die Mineralrevision wurde freigegeben und veröffentlicht.",
                    "Die Mineralrevision wurde abgelehnt.",
                ),
                Language::Fr => (
                    "La révision du minéral a été approuvée et publiée.",
                    "La révision du minéral a été rejetée.",
                ),
                Language::Pt => (
                    "A revisão do mineral foi aprovada e publicada.",
                    "A revisão do mineral foi rejeitada.",
                ),
                Language::Zh => ("矿物版本已批准并发布。", "矿物版本已拒绝。"),
                Language::Ar => (
                    "تمت الموافقة على نسخة المعدن ونشرها.",
                    "تم رفض نسخة المعدن.",
                ),
                Language::Hi => (
                    "खनिज संस्करण स्वीकृत और प्रकाशित किया गया।",
                    "खनिज संस्करण अस्वीकार किया गया।",
                ),
                Language::Ja => (
                    "鉱物の版を承認して公開しました。",
                    "鉱物の版を却下しました。",
                ),
            };

            assert_eq!(text.approved_notice, approved);
            assert_eq!(text.rejected_notice, rejected);
            assert_ne!(text.approved_notice.trim_end_matches('.'), text.approve);
            assert_ne!(text.rejected_notice.trim_end_matches('.'), text.reject);
        }
    }

    #[test]
    fn research_summary_copy_is_localized_without_process_jargon() {
        for &language in Language::all() {
            let expected = match language {
                Language::En => "Research summary",
                Language::Es => "Resumen de investigación",
                Language::Cs => "Shrnutí výzkumu",
                Language::De => "Forschungszusammenfassung",
                Language::Fr => "Synthèse de la recherche",
                Language::Pt => "Resumo da pesquisa",
                Language::Zh => "研究摘要",
                Language::Ar => "ملخص البحث",
                Language::Hi => "शोध सारांश",
                Language::Ja => "調査概要",
            };
            assert_eq!(ui_text(language).current_chain_output, expected);
        }
    }

    #[test]
    fn aliases_share_labels_and_unknown_keys_are_rejected() {
        for &language in Language::all() {
            assert_eq!(
                material_fact_label(language, "color"),
                material_fact_label(language, "colour")
            );
            assert_eq!(
                material_fact_label(language, "luster"),
                material_fact_label(language, "lustre")
            );
        }
        assert_eq!(material_fact_label(Language::En, " Colour "), Some("Color"));
        assert_eq!(material_fact_label(Language::En, "internal_key"), None);
        assert_eq!(material_fact_label(Language::En, ""), None);
    }

    #[test]
    fn scientific_labels_retain_units_in_every_language() {
        for &language in Language::all() {
            for key in ["boiling_point_c", "melting_point_c"] {
                assert!(material_fact_label(language, key)
                    .expect("temperature label")
                    .contains("°C"));
            }
            assert!(material_fact_label(language, "density_g_cm3")
                .expect("density label")
                .contains("g/cm³"));
            assert!(material_fact_label(language, "molar_mass_g_mol")
                .expect("molar mass label")
                .contains("g/mol"));
            assert!(material_fact_label(language, "major_elements_pct")
                .expect("percentage label")
                .contains('%'));
        }
    }
}
