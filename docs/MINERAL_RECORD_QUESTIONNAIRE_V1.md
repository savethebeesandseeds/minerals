# Mineral record questionnaire v1

Status: **draft contract for pilot validation**

Questionnaire ID: `waajacu-mineral-record-questionnaire-v1`

Population baseline: the 6,226 valid species in public release
`sha256:684c2830a145bfa0f0b0ecaf4f80bac64ddb51ffcb4a75035304840a868bccc4`

Machine-readable draft:
[`schemas/mineral-record-questionnaire-v1.json`](../schemas/mineral-record-questionnaire-v1.json)

Source decisions:
[`docs/MINERAL_SOURCE_ADMISSION_V1.md`](MINERAL_SOURCE_ADMISSION_V1.md) and
[`schemas/mineral-source-admission-matrix-v1.json`](../schemas/mineral-source-admission-matrix-v1.json)

## Purpose

This questionnaire defines what Waajacu means by a complete mineral record
before sources are selected or profile facts are imported. It is a versioned
field dictionary, applicability protocol, provenance contract, and public
presentation plan. It is not a prompt for a model to answer from memory and it
is not an import payload by itself.

The population is fixed for this effort. Profile enrichment must not add,
merge, withdraw, rename, or otherwise change the existing mineral identities.
Nomenclature updates remain separate, authority-owned releases.

Once the pilot freezes v1, a question key's scientific meaning, subject scope,
value schema, and canonical units are immutable. A semantic change requires a
new key or questionnaire version plus an explicit migration.

A complete record does not need a value for every question. It does need an
explicit, reviewable state for every eligible core question. A truthful
`not_reported_by_reviewed_sources` or `not_applicable` answer is more complete
than an unsupported value.

## Non-goals

Questionnaire v1 does not cover:

- photographs, diagrams, microscopy, or synthetic media;
- provider offers, prices, stock, grade, or other commercial claims;
- individual specimen inventory or specimen-specific measurements, except for
  reference-only links identifying formally designated type material;
- raw CIF, diffraction, spectral, or analytical files;
- exhaustive lists of every reported locality;
- global rarity, abundance, or probability rankings;
- geographic heat maps before occurrence coverage and reporting bias are
  documented;
- model-generated scientific identifiers or uncited scientific facts; or
- translated editorial descriptions. Controlled labels and sourced names may
  be localized independently.

## Governing principles

1. **Questions precede sources.** Stable meaning, scope, shape, units, and
   applicability are agreed before mapping any dataset.
2. **Sources own claims, not whole records.** A source may populate only the
   admitted question keys and may not overwrite unrelated claims.
3. **Claims are atomic and source-specific.** Different values remain separate;
   they are never silently averaged or overwritten.
4. **Subject scopes and production kinds stay explicit.** A fact about one mine
   or specimen is not promoted to the species profile without a source that
   explicitly supports the generalization, and a derived answer remains
   labelled derived.
5. **Missingness is data.** Empty strings, zero, empty objects, empty arrays,
   `unknown`, `N/A`, and `?` are not missing-state encodings.
6. **Conditions travel with measurements.** Ranges, units, methods, wavelength,
   temperature, sample state, uncertainty, and other qualifiers are part of the
   answer.
7. **Automation is mechanical, not epistemic.** Adapters may extract,
   normalize, validate, and diff. They do not turn model memory or an inference
   into scientific evidence.
8. **Consumer behavior is part of the contract.** A question definition states
   whether the answer is displayed, searchable, filterable, comparable,
   available to site tools, or subject to redaction.

## Orthogonal question metadata

Every question declares independent subject scope, value kind, production kind,
and priority tier. These axes must not be collapsed into one `scope` or `type`
field.

### Subject scope

Subject scope identifies what the answer is about:

| Scope | Meaning |
|---|---|
| `species` | A fact the source explicitly attributes to the mineral species. |
| `occurrence` | A fact about one reported locality, deposit, or occurrence. |
| `specimen` | An observation or measurement of one identified specimen. |
| `relationship` | A typed relationship between mineral identities. |

### Value kind

Value kind identifies the answer's data shape. The registry may add reusable
schema versions, but questionnaire v1 begins with `text`, `controlled_term`,
`boolean`, `integer`, `year_or_date`, `decimal`, `range`, `measurement`,
`formula`, `identifier`, `structured`, `material_relation`, and
`occurrence_relation`.

A measurement is therefore still species-, occurrence-, or specimen-scoped;
it is not a subject scope.

### Production kind

Every answer also declares its production kind:

- `sourced`: normalized from an admitted external source;
- `derived`: deterministically calculated from cited input claim IDs and a
  versioned algorithm;
- `editorial`: written from resolved reviewed claims and not itself evidence.

Production kind is independent from scientific review status. A derived or
editorial answer can be reviewed without becoming an externally sourced
measurement.

## Priority tiers

| Tier | Requirement |
|---|---|
| `core` | Once its module is active, every eligible mineral must have an explicit coverage state. |
| `conditional` | Once its module is active, applicability must be evaluated for every mineral; applicable cases require an explicit coverage state. |
| `extended` | Valuable specialist content, but not a core-profile completion gate. |
| `deferred` | Explicitly outside questionnaire v1. |

`derived` and `editorial` are production kinds, not priority tiers. A derived
answer may be core, conditional, or extended; an editorial answer is ordinarily
extended.

### Module lifecycle

Module rollout is separate from per-record coverage. Each module has one
lifecycle state:

| State | Meaning |
|---|---|
| `not_launched` | Source admission, schema, and research have not begun. |
| `pilot` | Definitions and mappings are being evaluated on the frozen pilot. |
| `active` | The module is being evaluated across the catalog. |
| `complete` | Its declared release gate has passed for the full population. |
| `superseded` | A newer module/questionnaire version replaces it. |

`not_launched` is never copied into 6,226 mineral answer rows as
`not_yet_researched`. Public consumers summarize or collapse a globally
unavailable module and explain its lifecycle. Per-record coverage begins only
when the module is `pilot` or `active` for that population.

## Answer and missing-state contract

Coverage, review, and conflict resolution are separate axes.

### Coverage state

| State | Meaning | Required context |
|---|---|---|
| `value_present` | One or more claims contain a usable value. | Claim IDs and provenance. |
| `not_yet_researched` | No adequate source review has occurred. | Reviewer or process actor and timestamp; no scientific citation is implied. |
| `not_reported_by_reviewed_sources` | Declared admitted sources were checked but did not report a value. | Checked source releases, search scope, reviewer, and timestamp. |
| `not_applicable` | The concept scientifically does not apply. | Versioned applicability rule and a reason code; evidence when non-obvious. |
| `withheld` | A value is known privately but cannot be published. | Public-safe restriction reason and retained private provenance. |

Source uncertainty belongs in the claim value, range, conditions, note, and
confidence. It must not be collapsed into a vague `unknown` string.

### Claim review state

Claim associations use the existing evidence vocabulary:

- `unreviewed` for a sourced claim not yet checked against its locator;
- `reviewed` for a claim checked against its source and locator;
- `verified` for a claim association that passed the project's stronger
  verification procedure; and
- `disputed` for a known scientific conflict or challenge.

Record-level verification remains a separate axis with the existing `draft`,
`generated`, `sourced`, `reviewed`, `verified`, and `disputed` states. Neither
record verification nor claim review duplicates production kind.

### Resolution state

| State | Meaning |
|---|---|
| `unresolved` | Claims exist but no public answer has been selected. |
| `single_supported` | One supported claim supplies the public answer. |
| `multiple_consistent` | Several compatible claims support the public answer. |
| `conflicting` | Claims disagree and no preferred public answer is selected. |
| `preferred_with_conflict` | A reviewer selected one public answer while preserving the disagreement and rationale. |

Conflicting values are never averaged. A preferred selection records the
reviewer, time, rationale, and all superseded or disagreeing claim IDs.

## Claim envelope

Every sourced value must be able to answer all of the following:

- Which mineral, question key, and subject scope does it concern?
- What normalized value is asserted, and what was the source's raw value?
- What unit, range, precision, uncertainty, method, and conditions apply?
- Which publisher, work, dataset, release, and source-owned record supplied it?
- What exact page, table, row, section, DOI, or record locator supports it?
- When were the raw bytes retrieved and what is their SHA-256?
- Which license, attribution, changes notice, and reuse restrictions apply?
- Which adapter, mapping, normalization, or derivation version produced it?
- What confidence, review state, reviewer, and review time apply?
- Does it conflict with another claim, and was a preferred value selected?

Coherent compound values remain atomic. For example, a unit cell is one answer
containing its axes, angles, `Z`, method, and conditions rather than seven
unrelated scalar claims.

## Question-definition metadata

Before ingestion, each question must have a machine-readable registry entry
covering:

- stable `question_key`, definition version, lifecycle, and replacement key;
- section, display order, short label, full question, and scientific
  definition;
- subject scope, priority tier, allowed production kinds, applicability rule,
  and allowed coverage states;
- value kind, cardinality, ordering, maximum items, canonical unit, accepted
  units, bounds, precision, controlled vocabulary/version, and value-schema
  version;
- raw-value preservation and normalization policy/version;
- minimum source class, locator requirement, condition/uncertainty requirement,
  review threshold, and conflict policy;
- public/private visibility and sensitivity/coarsening policy;
- renderer and placement on cards, record sections, comparisons, map popups,
  safety banners, and agent summaries;
- search mode, boost, facet type, filter order, sort/aggregation permission, and
  normalized index key; and
- public worker/site-tool key, response schema, ordering, missing-state
  contract, evidence inclusion, and compact-output priority.

Only controlled terms or canonical-unit measurements may become facets. Free
text may be searchable, but it must not silently generate filter categories.

## Source requirement codes

| Code | Minimum source requirement |
|---|---|
| `AUTH1` | One reviewed official authority or primary nomenclature source. |
| `SCI1` | One reviewed scholarly publication, authoritative handbook, or curated scientific database with traceable provenance. |
| `SCI2` | Two independent reviewed scientific sources supporting claims, with at least one associated claim at `review_status=verified`. |
| `SYN2` | Reviewer-authored synthesis from at least two granular scientific claims, with no unsupported new assertion. |
| `SAFE1` | A regulator, authoritative safety/radiological source, responsible-producer SDS, or peer-reviewed toxicology source appropriate to the asserted hazard. |
| `LOC1` | A source-owned locality, specimen, or deposit record, or a scholarly publication with an exact locality locator; coordinate provenance is additionally required whenever geometry is published. |
| `DER1` | A deterministic derivation from cited approved inputs with algorithm and version recorded. |
| `INT1` | Reviewer-authored editorial synthesis from recorded coverage, resolution, and conflict states; it may not introduce a new scientific assertion. |

Admission is per source and question group. A source accepted for
crystallography is not automatically accepted for safety, nomenclature, or
occurrence coordinates.

## Protected identity baseline

The following questions are already substantially populated by the IMA-owned
identity release. They remain in the completeness contract, but profile sources
must not re-own or overwrite them.

| Question key | Standard question | Tier | Shape | Source |
|---|---|---|---|---|
| `identity.canonical_name` | What is the authority-accepted name? | core | String | `AUTH1` |
| `identity.formula` | What formula does the nomenclature authority publish? | core | Formula string with source uncertainty preserved | `AUTH1` |
| `identity.nomenclature_status` | What is the normalized nomenclature state? | core | Controlled status | `AUTH1` |
| `identity.current_valid` | Is this currently a valid species in the baseline release? | core | Boolean | `AUTH1` |
| `identifiers.ima_number` | What IMA number is assigned, if any? | core | Zero or one identifier | `AUTH1` |
| `identifiers.ima_symbol` | What official IMA symbol is assigned, if any? | core | Zero or one identifier | `AUTH1` |
| `authority.discovery_country` | What country text does the authority provide as discovery context? | core | Zero or one source-preserved string | `AUTH1` |
| `authority.first_reference` | What first reference text does the authority provide? | core | Zero or one bibliographic string | `AUTH1` |
| `authority.second_reference` | What second reference text does the authority provide? | core | Zero or one bibliographic string | `AUTH1` |
| `authority.source_status` | What raw authority status code was supplied? | core | Controlled source code | `AUTH1` |

The official IMA symbol is an authority-owned question but does not come from
the July 2026 master-list PDF. It requires its own admitted authority
work/release before profile enrichment may populate it.

`authority.discovery_country` is source context, not a locality or occurrence.
It must never create a map point, area, country occurrence, or geographic
coverage claim.

## Names and classification

| Question key | Standard question | Tier | Shape | Source |
|---|---|---|---|---|
| `names.aliases` | Which accepted alternate names or spellings are reported? | core | Repeatable `{name, type, language?, origin}` | `AUTH1` or `SCI1` |
| `names.former_names` | Which former names are reported, and why did they change? | core | Repeatable name plus nomenclatural relationship and note | `AUTH1` or `SCI1` |
| `names.multilingual_names` | Which sourced language-specific names exist? | extended | Repeatable localized name with language and source | `SCI1` |
| `identifiers.cas_number` | Does an authoritative source assign a CAS Registry Number to this mineral species? | conditional | Zero or one checksum-valid identifier; never inferred from formula or a similarly named compound | `SCI1` |
| `classification.chemical_class` | What broad chemical class is assigned? | core | Controlled term with vocabulary version | `SCI1` |
| `classification.mineral_group` | What preferred group or family is assigned? | core | Controlled hierarchical term | `SCI1` |
| `classification.scheme_assignments` | How is the mineral classified under named, versioned schemes? | conditional | Repeatable `{scheme, version, code?, label, parent_path}` | `SCI1` |

An empty alias list from one source does not prove that no aliases exist.
Unless absence is explicitly established, use
`not_reported_by_reviewed_sources` after the admitted alias sources are checked.

## History and type information

| Question key | Standard question | Tier | Shape | Source |
|---|---|---|---|---|
| `history.discovery_year` | When was the mineral first discovered or recognized, if that event is documented? | core | `{year?, qualifier}` with exact/circa/before/after/ancient vocabulary | `AUTH1` or `SCI1` |
| `history.first_description_year` | In what year was the first formal description published? | core | `{year?, qualifier}` with exact/circa/before/after vocabulary | `AUTH1` or `SCI1` |
| `history.discovery_people` | Who discovered, described, or approved it, and in which role? | conditional | Repeatable `{name, role}` | `SCI1` |
| `history.name_etymology` | What is the sourced origin and meaning of the name? | core | Structured summary with named-for entities and language where known | `SCI1` |
| `history.first_description` | What is the primary description or approval publication? | core | Repeatable structured citation/DOI relation | `AUTH1` or `SCI1` |
| `history.type_localities` | Which reviewed occurrences are designated type localities? | core | Repeatable relation to occurrence IDs | `LOC1` |
| `history.type_material` | Is type material preserved, and in which repository or specimen record? | conditional | Repeatable repository/specimen relation | `SCI1` |
| `history.nomenclatural_notes` | What sourced history is needed to explain renaming, redefinition, or dispute? | extended | Sourced text | `SCI1` |

Type-locality display text is derived from occurrence records. It is not copied
into an independent untraceable species-level locality string.

## Chemistry and relationships

| Question key | Standard question | Tier | Shape | Source |
|---|---|---|---|---|
| `chemistry.formula_variants` | What ideal, structural, simplified, or observed formula variants are reported? | conditional | Repeatable `{formula, kind, qualifier?}` | `SCI1` |
| `chemistry.compositional_variation` | What non-stoichiometry or compositional variability is reported? | core | Structured sourced text/ranges | `SCI1` |
| `chemistry.substitutions` | Which substitutions are supported, and over what ranges or sites? | conditional | Repeatable structured substitution | `SCI1` |
| `chemistry.end_members` | Is the mineral an end member or related to named endpoints? | conditional | Typed material relations | `SCI1` |
| `chemistry.solid_solution_series` | Which solid-solution relationships and ranges are reported? | conditional | Typed relation with endpoints/range | `SCI1` |
| `chemistry.polymorphs` | Which polymorph relationships are reported? | conditional | Typed material relations | `SCI1` |
| `chemistry.polytypes` | Which polytypes or polytype designations are reported? | conditional | Typed material/designation relations | `SCI1` |
| `chemistry.related_species` | Which other scientifically meaningful mineral relationships are supported? | extended | Typed relation with controlled relationship kind | `SCI1` |
| `chemistry.essential_elements` | Which elements are deterministically essential in the applicable approved formula? | conditional | Derived ordered element-symbol list with input formula claim | `DER1` |
| `chemistry.molar_mass_g_mol` | Can molar mass be safely calculated from the approved formula? | extended | Derived decimal measurement in `g/mol` | `DER1` |
| `chemistry.major_elements_pct` | Can ideal mass percentages be safely calculated? | extended | Derived element-to-percent object, unit `% mass` | `DER1` |

Formula derivations run only when a versioned parser can prove the required
formula semantics. Variables, occupancies, ranges, mixed sites, uncertain
symbols, and other ambiguity block derivation unless the algorithm explicitly
supports them. Derived ideal composition is never presented as an observed
chemical analysis.

## Crystallography

| Question key | Standard question | Tier | Shape | Source |
|---|---|---|---|---|
| `crystallography.crystal_system` | What crystal system is reported? | core | Seven-system controlled term; `not_applicable` only for genuinely noncrystalline material | `SCI1` |
| `crystallography.point_group` | What crystal class or point group is reported? | core | Controlled Hermann–Mauguin term | `SCI1` |
| `crystallography.space_group` | What space group is reported? | core | Repeatable space-group observations with H–M/Hall/IT identifiers, setting context, consistency, and provenance | `SCI1` |
| `crystallography.unit_cell` | What unit-cell parameters and `Z` are reported, under what conditions? | extended | Repeatable complete six-parameter cells in angstrom/degree with uncertainty, reported volume, Z/formula context, method, and temperature/pressure conditions | `SCI1` |
| `crystallography.structure_references` | Which structural determinations support the profile? | extended | Repeatable structure-record citations with explicit roles, publication status, ordered authors, DOI or bibliography, and source linkage | `SCI1` |
| `crystallography.twinning` | What twinning laws or habits are reported? | conditional | Repeatable structured term plus source text | `SCI1` |

Separate source measurements of a unit cell remain separate claims. The
resolved profile may select a representative value only with an explicit rule
and reviewer rationale.

The three approved value contracts above are also the bounded scope of the
private [COD crystallography technical pilot](COD_CRYSTALLOGRAPHY_PILOT_V1.md).
That adapter contract does not launch the crystallography module, create
per-record coverage rows, admit COD as a public source, or authorize public
projection. The unit cell remains unavailable to the generic comparison
surface until a versioned multidimensional comparator is defined.

## Physical and diagnostic properties

| Question key | Standard question | Tier | Shape | Source |
|---|---|---|---|---|
| `physical.color` | Which natural colors and qualifiers are reported? | core | Repeatable controlled colors plus source text | `SCI1` |
| `physical.streak` | What streak colors are reported? | core | Repeatable controlled colors plus source text | `SCI1` |
| `physical.luster` | Which luster terms are reported? | core | Repeatable controlled terms | `SCI1` |
| `physical.transparency` | What transparency states are reported? | core | Controlled list: transparent, translucent, opaque, with qualifiers | `SCI1` |
| `physical.crystal_habit` | Which crystal habits or aggregate forms are reported? | core | Repeatable controlled terms plus source text | `SCI1` |
| `physical.tenacity` | What tenacity is reported? | core | Repeatable controlled terms | `SCI1` |
| `physical.cleavage` | What cleavage quality and directions are reported? | core | Repeatable `{quality, direction?, count?, angle_deg?}` | `SCI1` |
| `physical.parting` | What parting is reported? | conditional | Same structured shape as cleavage | `SCI1` |
| `physical.fracture` | What fracture types are reported? | core | Repeatable controlled terms plus source text | `SCI1` |
| `physical.hardness_mohs` | What Mohs hardness or range is reported, under what conditions? | core | Exact decimal or `{min,max}`, canonical unit `Mohs` | `SCI1` |
| `physical.density_measured_g_cm3` | What measured mass density values are reported? | core | Repeatable measurement/range in `g/cm3`, with method, temperature, and conditions | `SCI1` |
| `physical.specific_gravity` | What relative density or specific gravity is reported? | core | Repeatable unitless measurement/range with reference medium and temperature where supplied | `SCI1` |
| `physical.density_calculated_g_cm3` | What calculated density is reported? | conditional | Repeatable measurement/range in `g/cm3`, labelled calculated | `SCI1` or `DER1` |
| `physical.magnetism` | What magnetic response is reported, under what conditions? | conditional | Structured response and optional measurement | `SCI1` |
| `physical.fluorescence` | What fluorescence is reported for which excitation? | conditional | Repeatable excitation band/wavelength, color, intensity, and conditions | `SCI1` |
| `physical.phosphorescence` | What phosphorescence is reported, including duration and excitation? | conditional | Repeatable structured response | `SCI1` |
| `physical.radioactivity` | What radiological activity measurement is reported? | conditional | Physical measurement with canonical unit, method, specimen/material form, and conditions; no hazard interpretation | `SCI1` or `SAFE1` |
| `physical.other_diagnostic_behavior` | What other source-supported diagnostic behavior is reported? | extended | Typed uncommon property or sourced text | `SCI1` |
| `diagnostic.distinguishing_features` | Which features distinguish this species from commonly confused minerals? | extended | Editorial short statements linked to supporting resolved claims | `SYN2` |
| `diagnostic.common_confusions` | Which minerals can reasonably be confused with it, and why? | extended | Editorial typed mineral relations plus reviewed explanation | `SYN2` |

Color, density, hardness, and other variable properties are ranges or repeated
condition-specific claims. The public profile must not flatten them into a
false universal scalar.

## Optical properties

These questions are conditional. Applicability is evaluated from reviewed
optical characterization, not guessed from color or appearance.

| Question key | Standard question | Tier | Shape | Source |
|---|---|---|---|---|
| `optical.character` | Is the material isotropic, uniaxial, biaxial, or characterized only in reflected light? | conditional | Controlled term | `SCI1` |
| `optical.sign` | What optical sign is reported? | conditional | Controlled positive/negative/indeterminate term | `SCI1` |
| `optical.refractive_indices` | Which named refractive indices are reported, at which wavelength and temperature? | conditional | Named exact/range measurements with conditions | `SCI1` |
| `optical.birefringence` | What birefringence is reported? | conditional | Unitless exact/range with conditions | `SCI1` |
| `optical.optic_axis_angle_2v` | What measured or calculated `2V` is reported? | conditional | Exact/range in degree, with basis and conditions | `SCI1` |
| `optical.dispersion` | What dispersion value or qualified observation is reported? | conditional | Numeric or controlled qualified value with method | `SCI1` |
| `optical.pleochroism` | What pleochroic colors and directions are reported? | conditional | Repeatable `{direction?, color, intensity?}` | `SCI1` |
| `optical.reflected_light` | Which reflected-light properties are reported for opaque minerals? | extended | Schema-validated specialist measurement/narrative | `SCI1` |

## Geological context

Species-level geological answers require a source that explicitly generalizes
the context. Information reported only for one locality belongs on that
occurrence.

| Question key | Standard question | Tier | Shape | Source |
|---|---|---|---|---|
| `geology.formation_environments` | In which formation environments is the species reported to form? | core | Repeatable controlled terms plus sourced explanation | `SCI1` |
| `geology.deposit_types` | With which deposit or ore types is it associated? | conditional | Repeatable controlled terms | `SCI1` |
| `geology.host_rocks` | Which host rocks are supported at species level? | core | Repeatable controlled rock terms/references | `SCI1` |
| `geology.alteration` | Which alteration processes or products are reported? | conditional | Controlled terms plus sourced explanation | `SCI1` |
| `geology.paragenesis` | What paragenetic sequence or context is supported? | core | Structured sequence/association or sourced text | `SCI1` |
| `geology.associated_minerals` | Which species-level mineral associations are supported? | core | Typed material relations | `SCI1` |
| `geology.extraterrestrial_context` | Is a meteorite or other extraterrestrial context reported? | conditional | Controlled context plus occurrences/citations | `SCI1` |

## Occurrence and locality collection

Occurrence data is a one-to-many collection, not one latitude/longitude pair or
one country field on a mineral. Each occurrence must support the following
questions or explicit states:

| Occurrence field | Contract |
|---|---|
| `public_occurrence_id` | Stable unique public identifier; required. |
| `source_external_id` | Source-owned occurrence, locality, specimen, or deposit identifier when available. |
| `locality_name` | Source-preserved locality text; required. |
| `locality_type` | Controlled physical type such as mine, quarry, outcrop, deposit, district, or region. |
| `country_code` | ISO 3166-1 alpha-2 only when source resolution supports it. |
| `admin_areas` | Ordered optional administrative hierarchy with vocabulary/source context. |
| `basis` | Type locality, collected specimen, observed in place, mine/deposit record, literature report, or historical report. |
| `temporal_status` | Current, historical, or unknown state, distinct from missingness. |
| `event_date` | Partial date with stated precision and event kind. |
| `geometry_kind` | `point`, `area`, or `none`. |
| `geometry` | WGS84 point or reviewed area geometry; never silently geocoded from text. |
| `coordinate_precision_m` | Source-supported coordinate precision in metres. |
| `uncertainty_radius_m` | Nonnegative uncertainty for a point when known. |
| `coordinate_origin` | Source-supplied, calculated, or geocoded, with method/version where applicable. |
| `publication_transform` | Unchanged, coarsened, or withheld, with transformation/version and reason. |
| `sensitivity_policy` | Public, coarsened, or withheld; required. |
| `lifecycle_state` | Active, superseded, or withdrawn; temporal and evidence-review states remain separate. |
| `geological_context` | Optional occurrence-specific host rock, deposit type, formation, alteration, and paragenesis. |
| `associated_minerals` | Optional typed material relations specific to this occurrence. |
| `evidence` | `LOC1` source, exact locator, retrieval/hash/license snapshot, confidence, and review state; required. |
| `conflict_note` | Required when source locality or coordinate claims conflict. |

Species-level occurrence questions are:

| Question key | Standard question | Tier | Shape | Source |
|---|---|---|---|---|
| `occurrence.reviewed_reports` | Which reviewed, non-exhaustive occurrence reports are published? | extended | Repeatable occurrence relations | `LOC1` |
| `occurrence.geographic_summary` | What geographic summary follows from the currently reviewed occurrence set? | extended | Derived coverage-limited summary with input occurrence IDs | `DER1` |

Never average conflicting coordinates, invent missing precision, display more
precision than the source, or label the number of recorded reports as abundance
or probability.

Type locality is an occurrence basis/designation, not a physical locality type.
The coverage state and occurrence relations on `history.type_localities` are
the single species-level authority for whether type localities have been
evaluated.

## Safety

Safety questions are conservative and condition-specific. A chemical formula
may trigger private review, but element presence alone does not establish a
public toxicity or handling claim.

| Question key | Standard question | Tier | Shape | Source |
|---|---|---|---|---|
| `safety.radioactivity` | Is there authoritative radiological hazard guidance for this mineral or its relevant material form? | conditional | Hazard interpretation, exposure conditions, and controls linked to supporting physical measurements when available | `SAFE1` |
| `safety.toxicity` | Is toxicity supported for a relevant form and exposure route? | conditional | Classification, routes, affected material forms, conditions, and note | `SAFE1` |
| `safety.asbestiform_or_fibrous` | Is a hazardous fibrous/asbestiform habit supported, and under what conditions? | conditional | Structured habit, exposure context, and controls | `SAFE1` |
| `safety.dust_hazard` | Are cutting, grinding, powder, or inhalation hazards supported? | conditional | Particle/material conditions and controls | `SAFE1` |
| `safety.handling` | What handling controls are supported for collectors, laboratories, or industry? | core | Repeatable `{hazard, control, audience?, conditions?}` | `SAFE1` |
| `safety.storage_or_restrictions` | Are special storage, transport, disposal, or legal restrictions supported? | extended | Jurisdiction- and condition-specific structured guidance | `SAFE1` |

The absence of a published hazard answer is not a declaration of safety. The
public interface must distinguish `not_yet_researched` and
`not_reported_by_reviewed_sources` from an affirmative reviewed no-hazard
assessment.

`physical.radioactivity` owns measurements and their physical conditions.
`safety.radioactivity` owns only reviewed hazard interpretation and controls,
links to supporting measurement claims where available, and is the only one of
the two that may drive a public safety banner.

## Significance and editorial content

These questions are deliberately later-stage. They must not delay structured
scientific enrichment.

| Question key | Standard question | Tier | Shape | Source |
|---|---|---|---|---|
| `significance.scientific` | What documented scientific significance does the species have? | extended | Sourced statements | `SCI1` |
| `significance.industrial` | What documented industrial uses or research applications exist? | extended | Sourced, time-bounded statements | `SCI1` |
| `significance.gemological` | What documented gemological relevance exists? | extended | Sourced statements with material/specimen scope | `SCI1` |
| `significance.historical_cultural` | What documented historical or cultural significance exists? | extended | Sourced statements clearly separated from scientific claims | `SCI1` |
| `editorial.summary` | What concise description can be written from the resolved reviewed profile? | extended | Editorial source-language text with supporting claim IDs | `SYN2` |
| `editorial.limitations` | What important gaps, disputes, or coverage limits should readers see? | extended | Editorial source-language text derived from states/conflicts | `INT1` |

Questionnaire v1 intentionally has no generic `rarity` field. Geological
abundance, publication frequency, collectible availability, conservation
sensitivity, and market scarcity are different concepts and must not be
collapsed into one score.

## Storage and consumer implications

The current `identifiers_json`, `properties_json`, and `safety_json` containers
are useful for a pilot only when every question key has a schema-validated value
shape. They must not become an arbitrary JSON questionnaire dump.

The implementation should provide:

1. a versioned question-definition registry and explicit per-mineral coverage
   state;
2. source-specific claim storage that preserves raw values and conflicting
   assertions;
3. a reviewed resolution layer selecting the current public answer without
   deleting claims;
4. typed relations for aliases, classifications, mineral relationships, and
   occurrences;
5. reusable typed measurement/range/value schemas for comparable properties;
6. a source-owned profile release policy that cannot mutate IMA-owned identity
   fields;
7. a public schema version that exposes states, claims, relations, and
   occurrences without private review data; and
8. coordinated card, detail, search/facet, comparison, map, accessibility,
   public worker-query, and site-tool behavior.

The claim-scope validator currently recognizes only the legacy broad roots and
a small set of granular identity keys. Questionnaire implementation requires a
versioned expansion for every new namespace, an explicit legacy-scope
crosswalk, and resolution of the current documentation/code mismatch around
nomenclature claim scopes before accepting profile releases.

Answers that drive filters or safety warnings require typed normalized values.
Long-tail specialist notes may use schema-validated structured JSON and remain
detail-only. Safety banners must never depend on parsing prose.

In this document, public worker/query and site-tool contracts mean the exported
SQLite worker operations plus the browser's site-tool/WebMCP projection. They
do not introduce a new public server API.

### Legacy-to-v1 disposition

Existing fields and claims are preserved until a reviewed migration explicitly
applies the following disposition. The one generated legacy Phenakite profile
must not be silently upgraded into reviewed questionnaire answers.

| Current field or scope | Questionnaire v1 disposition |
|---|---|
| `canonical_name`, `formula`, `nomenclature_status`, `is_valid_species` | Project directly to the protected `identity.*` keys and retain IMA ownership. |
| `discovery_country`, `first_reference`, `second_reference`, `source_status` | Project directly to protected `authority.*` context; never reinterpret discovery country as occurrence. |
| `description` | Future public projection of `editorial.summary`; legacy/generated text remains labelled generated until rebuilt from resolved reviewed claims. |
| `mineral_family` | Future public projection of one reviewed preferred `classification.mineral_group`; retain scheme/version and all other assignments separately. |
| `cas_number` | Project to conditional `identifiers.cas_number`; checksum validation establishes syntax only, not mineral ownership. |
| `synonyms_json` and aliases | Migrate to typed `names.*`/alias relations with origin, language, and provenance. |
| `identifiers_json` | Migrate recognized `ima_number` and `ima_symbol` values only through their separately admitted authority sources; unmatched or arbitrary keys remain legacy data. |
| `properties.hardness_mohs` | Reviewed migration target is `physical.hardness_mohs`; legacy claims remain immutable under the old scope. |
| `properties.density_g_cm3` | Reviewed migration target is `physical.density_measured_g_cm3` only when the source establishes mass density; never reinterpret it as unitless specific gravity. |
| `properties.crystal_system` | Reviewed migration target is `crystallography.crystal_system`. |
| `properties.color`, `properties.streak`, `properties.luster` | Reviewed migration targets are their `physical.*` keys after controlled-vocabulary normalization. |
| `properties.major_elements_pct` | Reviewed migration target is `chemistry.major_elements_pct` only through an explicit `DER1` derivation from an approved formula; observed specimen analyses require a separate future question. |
| Other `properties_json` values | Require an explicit question-key/value-schema mapping; otherwise remain legacy data. |
| `safety_json` | Split into the applicable `safety.*` questions; no prose parsing or automatic hazard promotion. |
| Legacy `material_evidence` scopes | Remain immutable legacy claims. New v1 claims are emitted only by a reviewed migration/source adapter; scope aliases do not change scientific status. |
| `data_quality_score` | Deprecated as a completeness, trust, display, and search-ranking signal for public v2. Retain only as legacy audit data and replace it with per-module coverage/review/conflict metrics. |

The public-v2 consumer migration must update cards, record pages, search and
facets, comparison, worker responses, and site tools together so enriched facts
cannot exist only inside hidden JSON.

## Source admission matrix

Create one decision row per proposed **source × question group**:

| Column | Required decision |
|---|---|
| Publisher/work/release | Canonical publisher, title, URL, release/version, and date. |
| Intended ownership | Exact question keys the source may populate or support. |
| Authority fit | Nomenclature authority, primary measurement, curated reference, specialist database, locality source, safety authority, or other explicit role. |
| Subject/value/cardinality | Species, occurrence, specimen, or relationship scope; value kind; one or many claims. |
| Identity crosswalk | Stable source record ID and deterministic mapping to an existing mineral; never fuzzy/name/formula merging. |
| Coverage expectation | Eligible records/fields and a predeclared useful-yield expectation. |
| Format/access | JSON/CSV/XML/database preferred; HTML/PDF/manual extraction risks stated. |
| Locator quality | Source record ID, DOI, page, table, row, or section resolution. |
| Units/definitions | Published meanings, methods, conditions, and controlled vocabularies. |
| Update semantics | Rename, correction, deletion, replacement, cadence, and full/incremental behavior. |
| Rights | License, redistribution, attribution, derived-output compatibility, and restrictions. |
| Reproducibility | Frozen raw bytes, retrieval time, SHA-256, parser, configuration, and versions. |
| Conflict policy | Preferred, supporting, or conflict-only; never silent overwrite or averaging. |
| Review burden | Low/medium/high extraction and scientific risk plus required review sample. |
| Decision | Admitted, pilot-only, private-only, deferred, or rejected, with reason. |

Non-negotiable gates are an identifiable publisher/work/release, usable rights
for the intended public projection, frozen raw bytes and hash, stable source
identity or an explicitly reviewed crosswalk, exact locator capability, defined
units/semantics, documented update behavior, and a field-ownership boundary. A
weighted score cannot override a failed rights or identity gate.

The initial reviewed candidate rows and their evidence are maintained in the
[source admission decisions](MINERAL_SOURCE_ADMISSION_V1.md) and the normative
[machine-readable matrix](../schemas/mineral-source-admission-matrix-v1.json).
`pilot_only` never authorizes public claims. A bounded private adapter may begin
only after the source's publisher/work/release, rights, stable-identity, and
field-ownership gates pass; every remaining gate must pass before admission.

## Questionnaire pilot

The pilot validates question definitions, applicability rules, source mappings,
provenance, review workload, and public presentation. It is not evidence that
the entire catalog is already enriched.

### Reproducible 96-mineral sample

Use a mixed sample so random selection does not miss difficult cases and manual
selection does not overrepresent familiar minerals:

1. Encode every hash input as UTF-8 bytes using the stated ASCII salt, one
   literal zero byte (`0x00`), and the exact `public_id`. Sort by the 32 digest
   bytes ascending, then by UTF-8 `public_id` bytes as a tie-breaker.
2. Select 60 baseline records with salt `questionnaire-v1-baseline`. The first
   45 ranked IDs are the development baseline and the next 15 are the holdout
   baseline.
3. Add 36 predeclared challenge records from a versioned eligibility manifest.
   Each manifest entry contains `public_id`, challenge group/stratum, qualifying
   evidence, rule version, and curator. Freeze and hash this manifest before
   any normalized adapter output is inspected. Within each listed stratum,
   rank eligible IDs with salt
   `questionnaire-v1-challenge:<group>:<stratum>`, select the quota, skip IDs
   already selected, and top up with the next digest in that same stratum:
   - 12 nomenclature/status cases: two each from approved, grandfathered,
     renamed, redefined, questionable, and uncertain;
   - 12 value-shape/applicability cases covering complex or variable formulas,
     hydrates/grouped formulas, explicit formula uncertainty, missing authority
     identifiers, solid solutions/end members, polymorphs/polytypes, opaque,
     isotropic and anisotropic optical cases, measured-versus-calculated
     density, repeatable structured measurements, and genuine inapplicability;
     and
   - 12 provenance/risk cases covering conflicting values, historical naming or
     locality, sensitive/imprecise coordinates, radioactivity/toxicity/dust,
     identity-crosswalk anomalies, multilingual or diacritic-heavy names,
     source-page/table locators, unit conversions, and nontrivial
     normalization.
4. Within each of the three 12-record challenge groups, rank selected IDs with
   salt `questionnaire-v1-challenge-split:<group>`. The first nine are
   development and the remaining three are holdout. The complete split is
   therefore 72 development records (45 baseline + 27 challenge) and 24
   holdout records (15 baseline + 9 challenge).
5. Freeze question keys, vocabularies, applicability rules, source mappings,
   and adapter configuration after development and before processing holdout.
6. Compare every holdout value to its cited raw locator. Check each missing
   state against the audit context, applicability rule, or private restriction
   record that its state requires.

If holdout inspection causes a systemic mapping change, increment
`holdout_round`, exclude every previously inspected holdout ID, and choose 15
new baseline IDs plus three new IDs from each challenge group. Rank the
remaining eligible IDs with salts
`questionnaire-v1-holdout:<holdout_round>` and
`questionnaire-v1-challenge-holdout:<group>:<holdout_round>`. Keep the 72-record
development set fixed. If a challenge pool is exhausted, expand and re-hash its
eligibility manifest before inspecting any replacement output and record the
change in the pilot report.

Challenge criteria and quotas are fixed before normalized adapter output is
reviewed. Candidate sources may be used to nominate cases only after their
source-admission decisions are recorded.

### Review strategy

1. Resolve rights before adapter development.
2. Have a scientific reviewer approve each source-field mapping, unit
   conversion, vocabulary mapping, applicability rule, and derived calculation.
3. Review 100% of anomalies, conflicts, OCR/manual transformations, unit
   conversions, `not_applicable`, `withheld`, coordinates, safety claims, and
   derived values in the development set.
4. Review at least 20 examples per repetitive low-risk source-field mapping, or
   all examples when fewer than 20 exist.
5. Review 100% of holdout values against raw locators. Review
   `not_reported_by_reviewed_sources` against its checked source set and search
   scope, `not_applicable` against its rule/reason, `withheld` against its
   private restriction record, and `not_yet_researched` against module
   lifecycle plus actor/time audit context.
6. Require independent second review for safety, sensitive coordinates,
   identity crosswalk anomalies, conflict resolutions, and at least 10% of
   ordinary holdout claims.
7. Classify discrepancies as source, extraction, normalization, unit,
   applicability, identity, conflict, rights, or display errors, then preserve
   the adjudication record.

### Hard pilot acceptance gates

The pilot passes only when all of the following hold:

- exactly the same 6,226 mineral identities remain in scope;
- no IMA-owned name, formula, status, identifier, authority context, or alias is
  unintentionally changed;
- media and commerce are unchanged;
- every module included in final questionnaire-v1 acceptance is declared
  `pilot`; an interim subset pilot cannot claim final contract acceptance;
- 100% of core questions in declared pilot modules on all 96 records have
  explicit coverage states;
- 100% of conditional questions have an applicability decision; applicable
  questions have a valid coverage state, while inapplicable questions have
  `not_applicable` plus their versioned rule and reason;
- no normalized empty-string, zero-as-missing, literal `unknown`, `N/A`, or `?`
  placeholders are used as coverage states;
- `not_reported_by_reviewed_sources`, `not_applicable`, and `withheld` carry
  their required audit context;
- every published sourced value has a source/release, exact locator or source
  record ID, retrieval time, content hash, license/attribution, and claim review
  state;
- every normalization/conversion retains the raw value and transformation
  version;
- every derived value retains its input claim IDs and algorithm/version;
- every editorial value retains its supporting resolved claim IDs, synthesis
  policy/version, author or process actor, and reviewer;
- every public claim comes from an `admitted` source;
- every admitted source × question mapping meets its predeclared useful-yield
  gate on eligible pilot records or is reclassified/rejected before acceptance;
- no active pilot module passes solely by assigning all records
  `not_yet_researched`;
- there are zero critical holdout mismatches involving identity, sign,
  order-of-magnitude, units, coordinates/precision, hazard state, or false
  inapplicability;
- one semantic comparison unit is the complete normalized answer or coverage
  decision for one material, question key, and source mapping; repeatable items
  are judged together as that answer;
- initial holdout output reaches at least 99% semantic agreement overall and at
  least 95% for each source × question mapping with five or more comparison
  units; smaller mappings have no incorrect unit, and any repeated/systemic
  error blocks acceptance regardless of percentage;
- corrected, re-run post-adjudication holdout output reaches 100% agreement
  before release;
- all source conflicts remain visible and every preferred conflicting answer
  records reviewer and rationale;
- identical raw bytes plus mapping/config versions reproduce byte-identical
  canonical scientific payloads and hashes; the deterministic boundary excludes
  runtime database IDs and execution timestamps unless those values are frozen
  manifest inputs;
- identical reruns are idempotent and changed source/configuration produces a
  new release and diff;
- schema, vocabulary, unit, foreign-key, source-locator, public-export, and
  orphan checks pass; and
- all required reviews are attributable and all blocking anomalies are
  resolved.

The final pilot report records sample IDs, source decisions, coverage by module
and question, error classes, unresolved limitations, reviewer agreement and
adjudication, and the publication decision.

## Coverage reporting

Do not compress completeness into one opaque quality score. Report each module
and question separately:

- answer-state coverage: eligible records with an explicit state;
- sourced-value yield: eligible records with one or more admitted sourced
  values;
- cited-, reviewed-, and verified-value coverage;
- counts for each missing state and for disputed/resolution states;
- conflicts per 100 records;
- source release and retrieval freshness; and
- extraction, normalization, derivation, and manual-review rates.

Low source yield is a source-cost decision. It is not permission to invent a
value or relabel an unresearched question.

## Phased delivery

0. **Contract release:** approve questionnaire v1, controlled vocabularies,
   applicability/missing-state rules, source matrix, profile field-ownership
   policy, claim/resolution model, and public consumer requirements. No mineral
   content changes.
1. **Private pilot:** run the 96-record development/holdout protocol and publish
   its acceptance report. Do not auto-expand before it passes.
2. **Core profile release:** use an admitted broad structured source for
   classification, crystallography basics, and common physical properties.
   Publish explicit states for core questions in the modules activated by this
   release; globally `not_launched` modules remain summarized rather than
   generating mineral-level missing rows. Publish values only where supported.
3. **Specialist releases:** add chemistry relationships, detailed structure,
   optical properties, and supported safety claims one source-owned release at
   a time.
4. **History, type-locality, and geology release:** add discovery/description
   dates, etymology, species-level formation, host rock, deposit type,
   paragenesis, and associations. Publish the reviewed type-locality occurrence
   subset and its relations rather than duplicating type-locality text.
5. **Broader occurrence release:** publish additional reviewed one-to-many occurrences with
   coordinate precision, uncertainty/method, evidence basis, date, and
   sensitivity policy. Only then add occurrence points/areas and geographic
   filters to the public map.
6. **Editorial release:** generate concise source-language descriptions from
   resolved reviewed facts and localize controlled labels. Translation of
   editorial prose requires a later translation contract. Images remain
   separate.

Every phase is an immutable, source-pinned, reviewable release with its own
coverage/limitations report and rollback point. Later phases do not wait for
every optional earlier question to have a value.

## Contract-phase completion

Questionnaire v1 is ready for implementation when:

1. every key, subject scope, value kind, allowed production kinds, priority
   tier, value schema, unit, vocabulary, applicability rule, source threshold,
   conflict rule, and public consumer behavior is approved;
2. a machine-readable question registry mirrors this document and validates
   without duplicate or incompatible definitions;
3. the source-admission matrix template and reviewer responsibilities are
   approved;
4. the deterministic pilot selection can be reproduced from the frozen public
   release;
5. schema and ingestion proposals protect IMA-owned identity and support
   explicit coverage, source claims, resolution, relations, and occurrences;
6. public schema/UI/search/worker/site-tool plans show missing states and
   evidence instead of hiding absent JSON panels; and
7. no content adapter or fact import begins before its source-field ownership
   and rights decision is recorded.
