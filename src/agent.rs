use std::cmp::Ordering;

use chrono::Utc;

use crate::{
    i18n::Language,
    models::{Mineral, ReportRequest},
};

#[derive(Debug, Clone)]
pub struct ElementShare {
    pub name: String,
    pub percent: f32,
}

#[derive(Debug, Clone)]
pub struct MineralReport {
    pub mineral: Mineral,
    pub audience: String,
    pub purpose: String,
    pub site_context: String,
    pub generated_utc: String,
    pub dominant_element: String,
    pub dominant_element_pct: f32,
    pub hardness_band: String,
    pub density_band: String,
    pub summary: String,
    pub recommendations: Vec<String>,
    pub element_breakdown: Vec<ElementShare>,
}

#[derive(Debug, Clone)]
struct DerivedMetrics {
    dominant_element: String,
    dominant_element_pct: f32,
    hardness_band: HardnessBand,
    density_band: DensityBand,
    element_breakdown: Vec<ElementShare>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HardnessBand {
    Soft,
    Medium,
    Hard,
    VeryHard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DensityBand {
    Light,
    Moderate,
    Dense,
}

pub fn run_agentic_chain(
    mineral: &Mineral,
    request: &ReportRequest,
    language: Language,
) -> MineralReport {
    let metrics = derive_metrics(mineral, language);
    let summary = compose_summary(language, mineral, request, &metrics);
    let recommendations = propose_recommendations(language, mineral, request, &metrics);

    MineralReport {
        mineral: mineral.clone(),
        audience: request.audience.clone(),
        purpose: request.purpose.clone(),
        site_context: request.site_context.clone(),
        generated_utc: Utc::now().to_rfc3339(),
        dominant_element: metrics.dominant_element,
        dominant_element_pct: metrics.dominant_element_pct,
        hardness_band: localized_hardness_band(language, metrics.hardness_band).to_string(),
        density_band: localized_density_band(language, metrics.density_band).to_string(),
        summary,
        recommendations,
        element_breakdown: metrics.element_breakdown,
    }
}

fn derive_metrics(mineral: &Mineral, language: Language) -> DerivedMetrics {
    let mut element_breakdown: Vec<ElementShare> = mineral
        .major_elements_pct
        .iter()
        .map(|(name, percent)| ElementShare {
            name: name.clone(),
            percent: *percent,
        })
        .collect();

    element_breakdown.sort_by(|a, b| b.percent.partial_cmp(&a.percent).unwrap_or(Ordering::Equal));

    let dominant = element_breakdown.first().cloned().unwrap_or(ElementShare {
        name: localized_unknown(language).to_string(),
        percent: 0.0,
    });

    let hardness_band = match mineral.hardness_mohs {
        h if h < 3.0 => HardnessBand::Soft,
        h if h < 6.0 => HardnessBand::Medium,
        h if h < 7.5 => HardnessBand::Hard,
        _ => HardnessBand::VeryHard,
    };

    let density_band = match mineral.density_g_cm3 {
        d if d < 2.6 => DensityBand::Light,
        d if d < 3.2 => DensityBand::Moderate,
        _ => DensityBand::Dense,
    };

    DerivedMetrics {
        dominant_element: dominant.name,
        dominant_element_pct: dominant.percent,
        hardness_band,
        density_band,
        element_breakdown,
    }
}

fn compose_summary(
    language: Language,
    mineral: &Mineral,
    request: &ReportRequest,
    metrics: &DerivedMetrics,
) -> String {
    let hardness = localized_hardness_band(language, metrics.hardness_band);
    let density = localized_density_band(language, metrics.density_band);

    match language {
        Language::En => format!(
            "For {audience} and the {site} context, {mineral} is classified as {hardness} with {density} density behavior. The chemistry is led by {element} ({pct:.1} wt%), supporting {purpose} decisions.",
            audience = request.audience,
            site = request.site_context,
            mineral = mineral.common_name,
            hardness = hardness,
            density = density,
            element = metrics.dominant_element,
            pct = metrics.dominant_element_pct,
            purpose = request.purpose,
        ),
        Language::Es => format!(
            "Para {audience} y el contexto {site}, {mineral} se clasifica como {hardness} con comportamiento de densidad {density}. La quimica esta dominada por {element} ({pct:.1} % en peso), lo que respalda decisiones de {purpose}.",
            audience = request.audience,
            site = request.site_context,
            mineral = mineral.common_name,
            hardness = hardness,
            density = density,
            element = metrics.dominant_element,
            pct = metrics.dominant_element_pct,
            purpose = request.purpose,
        ),
        Language::Cs => format!(
            "Pro {audience} a kontext {site} je {mineral} klasifikovan jako {hardness} s {density} hustotnim chovanim. Chemii vede {element} ({pct:.1} hm. %), coz podporuje rozhodovani pro {purpose}.",
            audience = request.audience,
            site = request.site_context,
            mineral = mineral.common_name,
            hardness = hardness,
            density = density,
            element = metrics.dominant_element,
            pct = metrics.dominant_element_pct,
            purpose = request.purpose,
        ),
        Language::Zh => format!(
            "面向{audience}并结合{site}场景，{mineral}被判定为{hardness}，密度表现为{density}。其化学组成以{element}为主（{pct:.1} wt%），可支持{purpose}相关决策。",
            audience = request.audience,
            site = request.site_context,
            mineral = mineral.common_name,
            hardness = hardness,
            density = density,
            element = metrics.dominant_element,
            pct = metrics.dominant_element_pct,
            purpose = request.purpose,
        ),
        Language::Ar => format!(
            "بالنسبة الى {audience} وفي سياق {site}، يتم تصنيف {mineral} على انه {hardness} مع سلوك كثافة {density}. التركيب الكيميائي يهيمن عليه {element} بنسبة ({pct:.1} wt%)، ما يدعم قرارات {purpose}.",
            audience = request.audience,
            site = request.site_context,
            mineral = mineral.common_name,
            hardness = hardness,
            density = density,
            element = metrics.dominant_element,
            pct = metrics.dominant_element_pct,
            purpose = request.purpose,
        ),
        Language::Fr => format!(
            "Pour {audience} et le contexte {site}, {mineral} est classe comme {hardness} avec un comportement de densite {density}. La chimie est dominee par {element} ({pct:.1} wt%), ce qui soutient les decisions de {purpose}.",
            audience = request.audience,
            site = request.site_context,
            mineral = mineral.common_name,
            hardness = hardness,
            density = density,
            element = metrics.dominant_element,
            pct = metrics.dominant_element_pct,
            purpose = request.purpose,
        ),
        Language::De => format!(
            "Fur {audience} im Kontext {site} wird {mineral} als {hardness} mit {density} Dichteverhalten eingestuft. Die Chemie wird von {element} ({pct:.1} wt%) dominiert und unterstutzt Entscheidungen zu {purpose}.",
            audience = request.audience,
            site = request.site_context,
            mineral = mineral.common_name,
            hardness = hardness,
            density = density,
            element = metrics.dominant_element,
            pct = metrics.dominant_element_pct,
            purpose = request.purpose,
        ),
        Language::Pt => format!(
            "Para {audience} e no contexto {site}, {mineral} e classificado como {hardness} com comportamento de densidade {density}. A quimica e liderada por {element} ({pct:.1} wt%), apoiando decisoes de {purpose}.",
            audience = request.audience,
            site = request.site_context,
            mineral = mineral.common_name,
            hardness = hardness,
            density = density,
            element = metrics.dominant_element,
            pct = metrics.dominant_element_pct,
            purpose = request.purpose,
        ),
        Language::Hi => format!(
            "{audience} ke liye aur {site} sandarbh me, {mineral} ko {hardness} ke roop me vargit kiya gaya hai aur iski ghanatva pravrtti {density} hai. Rasayanik roop se {element} pramukh hai ({pct:.1} wt%), jo {purpose} nirnayon ko samarthan deta hai.",
            audience = request.audience,
            site = request.site_context,
            mineral = mineral.common_name,
            hardness = hardness,
            density = density,
            element = metrics.dominant_element,
            pct = metrics.dominant_element_pct,
            purpose = request.purpose,
        ),
        Language::Ja => format!(
            "{audience} 向けで {site} の文脈では、{mineral} は {hardness} に分類され、密度特性は {density} です。化学組成は {element}（{pct:.1} wt%）が優勢で、{purpose} の判断を支援します。",
            audience = request.audience,
            site = request.site_context,
            mineral = mineral.common_name,
            hardness = hardness,
            density = density,
            element = metrics.dominant_element,
            pct = metrics.dominant_element_pct,
            purpose = request.purpose,
        ),
    }
}

fn propose_recommendations(
    language: Language,
    mineral: &Mineral,
    request: &ReportRequest,
    metrics: &DerivedMetrics,
) -> Vec<String> {
    let mut recs = Vec::new();

    match language {
        Language::En => recs.push(format!(
            "Prioritize samples of {} where {} enrichment is strongest.",
            mineral.common_name, metrics.dominant_element
        )),
        Language::Es => recs.push(format!(
            "Priorice muestras de {} donde el enriquecimiento de {} sea mas fuerte.",
            mineral.common_name, metrics.dominant_element
        )),
        Language::Cs => recs.push(format!(
            "Uprednostnete vzorky {} tam, kde je obohaceni {} nejsilnejsi.",
            mineral.common_name, metrics.dominant_element
        )),
        Language::Zh => recs.push(format!(
            "优先采集 {} 中 {} 富集最明显的样品。",
            mineral.common_name, metrics.dominant_element
        )),
        Language::Ar => recs.push(format!(
            "اعط اولوية لعينات {} حيث يكون اغناء {} هو الاقوى.",
            mineral.common_name, metrics.dominant_element
        )),
        Language::Fr => recs.push(format!(
            "Priorisez les echantillons de {} la ou l'enrichissement en {} est le plus fort.",
            mineral.common_name, metrics.dominant_element
        )),
        Language::De => recs.push(format!(
            "Priorisieren Sie Proben von {} dort, wo die Anreicherung von {} am starksten ist.",
            mineral.common_name, metrics.dominant_element
        )),
        Language::Pt => recs.push(format!(
            "Priorize amostras de {} onde o enriquecimento de {} for mais forte.",
            mineral.common_name, metrics.dominant_element
        )),
        Language::Hi => recs.push(format!(
            "{} ke un samples ko prathmikta dein jahan {} ka enrichment sabse adhik ho.",
            mineral.common_name, metrics.dominant_element
        )),
        Language::Ja => recs.push(format!(
            "{} では {} の濃集が最も強いサンプルを優先してください。",
            mineral.common_name, metrics.dominant_element
        )),
    }

    if matches!(
        metrics.hardness_band,
        HardnessBand::VeryHard | HardnessBand::Hard
    ) {
        recs.push(match language {
            Language::En => {
                "Use abrasion-resistant tooling and adjust comminution energy estimates upward.".to_string()
            }
            Language::Es => {
                "Use herramientas resistentes a la abrasion y ajuste al alza las estimaciones de energia de conminucion.".to_string()
            }
            Language::Cs => {
                "Pouzijte oteruvzdorne nastroje a navyste odhady energie drceni a mleti.".to_string()
            }
            Language::Zh => "使用耐磨工具，并上调粉碎能耗估算。".to_string(),
            Language::Ar => {
                "استخدم ادوات مقاومة للتآكل وارفع تقديرات طاقة التكسير والطحن.".to_string()
            }
            Language::Fr => {
                "Utilisez des outils resistants a l'abrasion et revoyez a la hausse les estimations d'energie de comminution.".to_string()
            }
            Language::De => {
                "Verwenden Sie abriebfeste Werkzeuge und erhohen Sie die Schatzungen fur den Zerkleinerungsenergiebedarf.".to_string()
            }
            Language::Pt => {
                "Use ferramentas resistentes a abrasao e aumente as estimativas de energia de cominuicao.".to_string()
            }
            Language::Hi => {
                "Abrasion-resistant tooling ka upyog karein aur comminution energy ke andazon ko badhayein.".to_string()
            }
            Language::Ja => {
                "耐摩耗工具を使用し、粉砕エネルギー見積もりを上方修正してください。".to_string()
            }
        });
    } else {
        recs.push(match language {
            Language::En => {
                "Validate breakage and weathering rates early, as softer material can bias grade control.".to_string()
            }
            Language::Es => {
                "Valide temprano las tasas de fractura y meteorizacion, ya que el material mas blando puede sesgar el control de ley.".to_string()
            }
            Language::Cs => {
                "Vcas overte miru rozpadu a zvetravani, protoze mekci material muze zkreslit kontrolu kvality.".to_string()
            }
            Language::Zh => "尽早验证破碎与风化速率，较软物料可能导致品位控制偏差。".to_string(),
            Language::Ar => {
                "تحقق مبكرا من معدلات التفتت والتجوية، لان المادة الاكثر ليونة قد تسبب انحيازا في ضبط العيار.".to_string()
            }
            Language::Fr => {
                "Validez tot les taux de fragmentation et d'alteration, car un materiau plus tendre peut biaiser le controle de teneur.".to_string()
            }
            Language::De => {
                "Prufen Sie fruhzeitig Bruch- und Verwitterungsraten, da weicheres Material die Gehaltskontrolle verzerren kann.".to_string()
            }
            Language::Pt => {
                "Valide cedo as taxas de fratura e intemperismo, pois material mais macio pode enviesar o controle de teor.".to_string()
            }
            Language::Hi => {
                "Breakage aur weathering rates ko shuruaat me validate karein, kyunki naram material grade control ko bias kar sakta hai.".to_string()
            }
            Language::Ja => {
                "軟質な鉱物は品位管理を偏らせる可能性があるため、破砕性と風化速度を早期に検証してください。".to_string()
            }
        });
    }

    if metrics.density_band == DensityBand::Dense {
        recs.push(match language {
            Language::En => {
                "Run density separation testwork to confirm recovery uplift potential in early flowsheets.".to_string()
            }
            Language::Es => {
                "Realice pruebas de separacion por densidad para confirmar el potencial de mejora de recuperacion en los flowsheets iniciales.".to_string()
            }
            Language::Cs => {
                "Provedte testy hustotni separace pro potvrzeni potencialu navyseni vytaznosti v ranem navrhu technologie.".to_string()
            }
            Language::Zh => "开展密度分选试验，以确认早期流程中回收率提升潜力。".to_string(),
            Language::Ar => {
                "نفذ اختبارات الفصل بالكثافة لتاكيد امكانية رفع الاسترداد في مخططات المعالجة المبكرة.".to_string()
            }
            Language::Fr => {
                "Realisez des essais de separation par densite pour confirmer le potentiel de gain de recuperation dans les premiers flowsheets.".to_string()
            }
            Language::De => {
                "Fuhren Sie Dichtetrennversuche durch, um das Potenzial fur bessere Ausbringung in fruhen Flowsheets zu bestatigen.".to_string()
            }
            Language::Pt => {
                "Execute testes de separacao por densidade para confirmar o potencial de aumento de recuperacao nos flowsheets iniciais.".to_string()
            }
            Language::Hi => {
                "Early flowsheets me recovery uplift potential ki pushti ke liye density separation testwork chalayein.".to_string()
            }
            Language::Ja => {
                "初期フローシートにおける回収率向上の可能性を確認するため、比重選別試験を実施してください。".to_string()
            }
        });
    } else {
        recs.push(match language {
            Language::En => {
                "Combine XRD with geochemistry to avoid over-reliance on density-based separation.".to_string()
            }
            Language::Es => {
                "Combine XRD con geoquimica para evitar una dependencia excesiva de la separacion basada en densidad.".to_string()
            }
            Language::Cs => {
                "Kombinujte XRD s geochemii, aby se predeslo nadmernemu spolihani na hustotni separaci.".to_string()
            }
            Language::Zh => "将 XRD 与地球化学结合，避免过度依赖基于密度的分选。".to_string(),
            Language::Ar => {
                "ادمج XRD مع الجيوكيمياء لتجنب الاعتماد المفرط على الفصل المعتمد على الكثافة.".to_string()
            }
            Language::Fr => {
                "Combinez la DRX (XRD) avec la geochimie pour eviter une dependance excessive a la separation par densite.".to_string()
            }
            Language::De => {
                "Kombinieren Sie XRD mit Geochemie, um eine ubermassige Abhangigkeit von dichtebasierter Trennung zu vermeiden.".to_string()
            }
            Language::Pt => {
                "Combine XRD com geoquimica para evitar dependencia excessiva da separacao baseada em densidade.".to_string()
            }
            Language::Hi => {
                "Density-based separation par adhik nirbharata se bachne ke liye XRD ko geochemistry ke saath jodiye.".to_string()
            }
            Language::Ja => {
                "比重分離への過度な依存を避けるため、XRD と地球化学データを組み合わせて評価してください。".to_string()
            }
        });
    }

    match language {
        Language::En => recs.push(format!(
            "Archive this report against '{}' objectives for reproducible decision records.",
            request.purpose
        )),
        Language::Es => recs.push(format!(
            "Archive este informe bajo los objetivos de '{}' para mantener registros de decision reproducibles.",
            request.purpose
        )),
        Language::Cs => recs.push(format!(
            "Archivujte tuto zpravu k cilum '{}' pro reprodukovatelny rozhodovaci zaznam.",
            request.purpose
        )),
        Language::Zh => recs.push(format!(
            "请将本报告归档到“{}”目标下，以保留可复现的决策记录。",
            request.purpose
        )),
        Language::Ar => recs.push(format!(
            "ارشِف هذا التقرير ضمن اهداف '{}' للحفاظ على سجل قرارات قابل لاعادة التتبع.",
            request.purpose
        )),
        Language::Fr => recs.push(format!(
            "Archivez ce rapport sous les objectifs '{}' pour conserver des traces de decision reproductibles.",
            request.purpose
        )),
        Language::De => recs.push(format!(
            "Archivieren Sie diesen Bericht unter den Zielen '{}' fur reproduzierbare Entscheidungsnachweise.",
            request.purpose
        )),
        Language::Pt => recs.push(format!(
            "Arquive este relatorio sob os objetivos '{}' para manter registros de decisao reproduziveis.",
            request.purpose
        )),
        Language::Hi => recs.push(format!(
            "Punrutrutpann nirnay records ke liye is report ko '{}' uddeshyon ke saath archive karein.",
            request.purpose
        )),
        Language::Ja => recs.push(format!(
            "再現可能な意思決定記録のため、このレポートを '{}' の目的に紐づけて保存してください。",
            request.purpose
        )),
    }

    recs
}

fn localized_unknown(language: Language) -> &'static str {
    match language {
        Language::En => "Unknown",
        Language::Es => "Desconocido",
        Language::Cs => "Nezname",
        Language::Zh => "未知",
        Language::Ar => "غير معروف",
        Language::Fr => "Inconnu",
        Language::De => "Unbekannt",
        Language::Pt => "Desconhecido",
        Language::Hi => "Agyat",
        Language::Ja => "不明",
    }
}

fn localized_hardness_band(language: Language, band: HardnessBand) -> &'static str {
    match language {
        Language::En => match band {
            HardnessBand::Soft => "soft",
            HardnessBand::Medium => "medium",
            HardnessBand::Hard => "hard",
            HardnessBand::VeryHard => "very hard",
        },
        Language::Es => match band {
            HardnessBand::Soft => "blando",
            HardnessBand::Medium => "medio",
            HardnessBand::Hard => "duro",
            HardnessBand::VeryHard => "muy duro",
        },
        Language::Cs => match band {
            HardnessBand::Soft => "mekky",
            HardnessBand::Medium => "stredni",
            HardnessBand::Hard => "tvrdy",
            HardnessBand::VeryHard => "velmi tvrdy",
        },
        Language::Zh => match band {
            HardnessBand::Soft => "较软",
            HardnessBand::Medium => "中等",
            HardnessBand::Hard => "较硬",
            HardnessBand::VeryHard => "很硬",
        },
        Language::Ar => match band {
            HardnessBand::Soft => "لين",
            HardnessBand::Medium => "متوسط",
            HardnessBand::Hard => "صلب",
            HardnessBand::VeryHard => "شديد الصلابة",
        },
        Language::Fr => match band {
            HardnessBand::Soft => "tendre",
            HardnessBand::Medium => "moyen",
            HardnessBand::Hard => "dur",
            HardnessBand::VeryHard => "tres dur",
        },
        Language::De => match band {
            HardnessBand::Soft => "weich",
            HardnessBand::Medium => "mittel",
            HardnessBand::Hard => "hart",
            HardnessBand::VeryHard => "sehr hart",
        },
        Language::Pt => match band {
            HardnessBand::Soft => "macio",
            HardnessBand::Medium => "medio",
            HardnessBand::Hard => "duro",
            HardnessBand::VeryHard => "muito duro",
        },
        Language::Hi => match band {
            HardnessBand::Soft => "naram",
            HardnessBand::Medium => "madhyam",
            HardnessBand::Hard => "kathor",
            HardnessBand::VeryHard => "bahut kathor",
        },
        Language::Ja => match band {
            HardnessBand::Soft => "軟らかい",
            HardnessBand::Medium => "中程度",
            HardnessBand::Hard => "硬い",
            HardnessBand::VeryHard => "非常に硬い",
        },
    }
}

fn localized_density_band(language: Language, band: DensityBand) -> &'static str {
    match language {
        Language::En => match band {
            DensityBand::Light => "light",
            DensityBand::Moderate => "moderate",
            DensityBand::Dense => "dense",
        },
        Language::Es => match band {
            DensityBand::Light => "ligero",
            DensityBand::Moderate => "moderado",
            DensityBand::Dense => "denso",
        },
        Language::Cs => match band {
            DensityBand::Light => "lehky",
            DensityBand::Moderate => "stredni",
            DensityBand::Dense => "husty",
        },
        Language::Zh => match band {
            DensityBand::Light => "较轻",
            DensityBand::Moderate => "中等",
            DensityBand::Dense => "致密",
        },
        Language::Ar => match band {
            DensityBand::Light => "خفيف",
            DensityBand::Moderate => "متوسط",
            DensityBand::Dense => "كثيف",
        },
        Language::Fr => match band {
            DensityBand::Light => "leger",
            DensityBand::Moderate => "modere",
            DensityBand::Dense => "dense",
        },
        Language::De => match band {
            DensityBand::Light => "leicht",
            DensityBand::Moderate => "moderat",
            DensityBand::Dense => "dicht",
        },
        Language::Pt => match band {
            DensityBand::Light => "leve",
            DensityBand::Moderate => "moderado",
            DensityBand::Dense => "denso",
        },
        Language::Hi => match band {
            DensityBand::Light => "halka",
            DensityBand::Moderate => "madhyam",
            DensityBand::Dense => "ghan",
        },
        Language::Ja => match band {
            DensityBand::Light => "軽い",
            DensityBand::Moderate => "中程度",
            DensityBand::Dense => "高密度",
        },
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::run_agentic_chain;
    use crate::{
        i18n::Language,
        models::{Mineral, ReportRequest},
    };

    #[test]
    fn chain_sorts_elements_and_sets_dominant() {
        let mut elements = BTreeMap::new();
        elements.insert("Si".to_string(), 46.7);
        elements.insert("O".to_string(), 53.3);

        let mineral = Mineral {
            slug: "mineral.silicate.0xaaaaaa".to_string(),
            folder_name: "mineral.silicate.0xaaaaaa".to_string(),
            common_name: "Test Mineral".to_string(),
            description: "Test description".to_string(),
            mineral_family: "silicate".to_string(),
            formula: "SiO2".to_string(),
            hardness_mohs: 7.0,
            density_g_cm3: 2.65,
            crystal_system: "trigonal".to_string(),
            color: "colorless".to_string(),
            streak: "white".to_string(),
            luster: "vitreous".to_string(),
            major_elements_pct: elements,
            notes: "n/a".to_string(),
            image_path: None,
        };

        let report = run_agentic_chain(&mineral, &ReportRequest::default(), Language::En);

        assert_eq!(report.dominant_element, "O");
        assert_eq!(report.element_breakdown[0].name, "O");
        assert_eq!(report.hardness_band, "hard");
    }
}
