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
pub struct UiText {
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

fn en_text() -> UiText {
    UiText {
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
        home_subtitle: "Select your language and continue to the mineral catalog.",
        home_select_language: "Language",
        home_continue: "Continue",

        catalog_title: "Minerals Catalog",
        catalog_subtitle: "Structured mineral records with reproducible HTML/PDF reporting.",
        no_minerals: "No minerals currently published. Open /admin to create the first entry.",
        open_mineral: "Open Mineral",

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
        report_builder_subtitle: "Generate report artifacts directly inside this mineral folder.",
        generate_pdf: "Generate PDF",
        status_pdf: "PDF",
        status_html: "HTML",
        status_pdf_failed: "PDF generation failed.",
        current_chain_output: "Current Chain Output",
        recommendations_heading: "Recommendations",

        about_title: "About Minerals",
        about_subtitle: "Folder-backed catalog and report platform focused on traceability and controlled publishing.",
        about_operating_model: "Operating Model",
        about_operating_body: "Each mineral is stored as a standalone folder record. Admin operators create and review drafts before publishing.",
        about_path_note: "Path convention: data/minerals/mineral.<family>.0x<id>",

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
    let mut t = en_text();

    match lang {
        Language::En => {}
        Language::Es => {
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
            t.catalog_subtitle = "Registros estructurados con informes HTML/PDF reproducibles.";
            t.no_minerals = "No hay minerales publicados. Abre /admin para crear el primero.";
            t.open_mineral = "Abrir mineral";
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
            t.report_builder_subtitle =
                "Genera artefactos de informe directamente en esta carpeta.";
            t.generate_pdf = "Generar PDF";
            t.status_pdf_failed = "Falló la generación de PDF.";
            t.current_chain_output = "Salida actual de la cadena";
            t.recommendations_heading = "Recomendaciones";
            t.about_title = "Acerca de Minerals";
            t.about_subtitle =
                "Plataforma de catálogo e informes con trazabilidad y publicación controlada.";
            t.about_operating_model = "Modelo operativo";
            t.about_operating_body = "Cada mineral se guarda como carpeta independiente. Los administradores revisan antes de publicar.";
            t.about_path_note = "Convención de ruta: data/minerals/mineral.<family>.0x<id>";
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
            t.catalog_subtitle = "Strukturované záznamy s reprodukovatelnými HTML/PDF reporty.";
            t.no_minerals = "Zatím nejsou publikovány žádné minerály. Otevřete /admin.";
            t.open_mineral = "Otevřít minerál";
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
            t.current_chain_output = "Aktuální výstup";
            t.recommendations_heading = "Doporučení";
            t.about_title = "O Minerals";
            t.about_subtitle =
                "Katalog a reporty se zaměřením na dohledatelnost a kontrolované publikování.";
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
            t.catalog_subtitle = "结构化矿物记录，支持可复现 HTML/PDF 报告。";
            t.no_minerals = "当前没有已发布矿物。请打开 /admin 创建第一条记录。";
            t.open_mineral = "打开矿物";
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
            t.report_builder_subtitle = "在当前矿物目录中直接生成报告文件。";
            t.generate_pdf = "生成 PDF";
            t.status_pdf = "PDF";
            t.status_html = "HTML";
            t.status_pdf_failed = "PDF 生成失败。";
            t.current_chain_output = "当前分析输出";
            t.recommendations_heading = "建议";
            t.about_title = "关于 Minerals";
            t.about_subtitle = "基于文件夹的矿物目录与报告平台，强调可追溯和受控发布。";
            t.about_operating_model = "运行模式";
            t.about_operating_body = "每个矿物保存为独立目录。管理员先创建并审核草稿，再发布。";
            t.about_path_note = "路径规范：data/minerals/mineral.<family>.0x<id>";
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
            t.catalog_subtitle = "سجلات منظمة مع تقارير HTML/PDF قابلة لإعادة الإنتاج.";
            t.no_minerals = "لا توجد معادن منشورة حالياً. افتح /admin لإنشاء أول سجل.";
            t.open_mineral = "فتح المعدن";
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
            t.report_builder_subtitle = "إنشاء ملفات التقرير مباشرة داخل مجلد المعدن.";
            t.generate_pdf = "إنشاء PDF";
            t.status_pdf = "PDF";
            t.status_html = "HTML";
            t.status_pdf_failed = "فشل إنشاء PDF.";
            t.current_chain_output = "المخرجات الحالية";
            t.recommendations_heading = "التوصيات";
            t.about_title = "حول Minerals";
            t.about_subtitle = "منصة فهرسة وتقارير قائمة على المجلدات مع تتبع ونشر مضبوط.";
            t.about_operating_model = "نموذج التشغيل";
            t.about_operating_body =
                "يُحفظ كل معدن في مجلد مستقل. ينشئ المسؤولون المسودات ويراجعونها قبل النشر.";
            t.about_path_note = "نمط المسار: data/minerals/mineral.<family>.0x<id>";
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
            t.catalog_subtitle =
                "Enregistrements structurés avec rapports HTML/PDF reproductibles.";
            t.no_minerals = "Aucun minéral publié. Ouvrez /admin pour créer le premier.";
            t.open_mineral = "Ouvrir le minéral";
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
            t.current_chain_output = "Sortie actuelle";
            t.recommendations_heading = "Recommandations";
            t.about_title = "À propos de Minerals";
            t.about_subtitle = "Plateforme de catalogue et de rapports axée sur la traçabilité.";
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
            t.nav_home = "Start";
            t.nav_all_minerals = "Alle Minerale";
            t.nav_about = "Über uns";
            t.nav_login = "anmelden";
            t.home_title = "Minerale";
            t.home_subtitle = "Sprache wählen und zum Mineralkatalog wechseln.";
            t.home_select_language = "Sprache";
            t.home_continue = "Weiter";
            t.catalog_title = "Mineralkatalog";
            t.catalog_subtitle =
                "Strukturierte Datensätze mit reproduzierbaren HTML/PDF-Berichten.";
            t.no_minerals = "Noch keine Minerale veröffentlicht. Öffnen Sie /admin.";
            t.open_mineral = "Mineral öffnen";
            t.label_family = "Familie";
            t.label_description = "Beschreibung";
            t.label_crystal_system = "Kristallsystem";
            t.label_notes = "Notizen";
            t.mineral_profile = "Mineralprofil";
            t.report_builder = "Berichtsgenerator";
            t.generate_pdf = "PDF erzeugen";
            t.status_pdf_failed = "PDF-Erzeugung fehlgeschlagen.";
            t.recommendations_heading = "Empfehlungen";
            t.about_title = "Über Minerals";
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
            t.nav_home = "Início";
            t.nav_all_minerals = "Todos os minerais";
            t.nav_about = "Sobre";
            t.nav_login = "entrar";
            t.home_title = "Minerais";
            t.home_subtitle = "Selecione o idioma e continue para o catálogo.";
            t.home_select_language = "Idioma";
            t.home_continue = "Continuar";
            t.catalog_title = "Catálogo de minerais";
            t.catalog_subtitle = "Registros estruturados com relatórios HTML/PDF reproduzíveis.";
            t.no_minerals = "Nenhum mineral publicado. Abra /admin para criar o primeiro.";
            t.open_mineral = "Abrir mineral";
            t.label_family = "Família";
            t.label_description = "Descrição";
            t.label_crystal_system = "Sistema cristalino";
            t.label_notes = "Notas";
            t.mineral_profile = "Perfil do mineral";
            t.report_builder = "Gerador de relatório";
            t.generate_pdf = "Gerar PDF";
            t.status_pdf_failed = "Falha ao gerar PDF.";
            t.recommendations_heading = "Recomendações";
            t.about_title = "Sobre o Minerals";
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
            t.nav_home = "होम";
            t.nav_all_minerals = "सभी खनिज";
            t.nav_about = "परिचय";
            t.nav_login = "लॉगिन";
            t.home_title = "मिनरल्स";
            t.home_subtitle = "भाषा चुनें और खनिज कैटलॉग में जाएँ।";
            t.home_select_language = "भाषा";
            t.home_continue = "आगे बढ़ें";
            t.catalog_title = "खनिज कैटलॉग";
            t.catalog_subtitle = "संरचित रिकॉर्ड और पुनरुत्पाद्य HTML/PDF रिपोर्ट।";
            t.no_minerals = "अभी कोई प्रकाशित खनिज नहीं है। /admin खोलें।";
            t.open_mineral = "खनिज खोलें";
            t.label_family = "परिवार";
            t.label_description = "विवरण";
            t.label_notes = "टिप्पणियाँ";
            t.mineral_profile = "खनिज प्रोफ़ाइल";
            t.major_composition = "मुख्य रासायनिक संरचना";
            t.computed_classification = "गणना-आधारित वर्गीकरण";
            t.report_builder = "रिपोर्ट बिल्डर";
            t.generate_pdf = "PDF बनाएँ";
            t.status_pdf_failed = "PDF निर्माण विफल हुआ।";
            t.current_chain_output = "वर्तमान आउटपुट";
            t.recommendations_heading = "सिफारिशें";
            t.about_title = "Minerals के बारे में";
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
            t.nav_home = "ホーム";
            t.nav_all_minerals = "全鉱物";
            t.nav_about = "概要";
            t.nav_login = "ログイン";
            t.home_title = "ミネラル";
            t.home_subtitle = "言語を選択して鉱物カタログへ進みます。";
            t.home_select_language = "言語";
            t.home_continue = "続行";
            t.catalog_title = "鉱物カタログ";
            t.catalog_subtitle = "再現可能な HTML/PDF レポートを備えた構造化レコード。";
            t.no_minerals = "公開済みの鉱物はありません。/admin で作成してください。";
            t.open_mineral = "鉱物を開く";
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
            t.current_chain_output = "現在の出力";
            t.recommendations_heading = "推奨事項";
            t.about_title = "Minerals について";
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
