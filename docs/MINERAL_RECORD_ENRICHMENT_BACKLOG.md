# Mineral record enrichment backlog

> **Active content effort:** use the versioned
> [mineral record questionnaire v1](MINERAL_RECORD_QUESTIONNAIRE_V1.md) to
> enrich the fixed mineral population through coordinated, source-owned,
> reviewed releases instead of adding isolated fields ad hoc.
> The first rights-and-fit review is recorded in the
> [source admission decisions](MINERAL_SOURCE_ADMISSION_V1.md); only its
> admitted source may currently supply new questionnaire-v1 public claims.
> Occurrence/locality data remains the first map-aware addition. Images remain
> a separate later project with their own provenance and asset pipeline.

The [record-by-record research workbench](MINERAL_RECORD_RESEARCH_WORKFLOW.md)
tracks the whole fixed population and preserves source-linked private drafts.
Codex owns this research and draft-writing effort while source pilots proceed.
Research progress is reported separately from reviewed and published content.

## Working direction — 6 October 2026

Preserve useful evidence and keep research moving. The questionnaire defines
the intended structured public record; it is not an exhaustive limit on what
we may investigate or retain. Private research and the already-approved COD
technical pilot can proceed while the broader registry remains draft.

Keep source-linked raw values, partial observations, conflicts, uncertain
matches, and useful data outside the current questionnaire. Preserve their
scope and unresolved questions instead of forcing them into an unsuitable
field. Synthetic, theoretical, duplicate, historical, and superseded records
can also have research value when labelled accordingly. Rejecting a proposed
mineral match rejects that association, not the source record. If source
material cannot be captured or reused, retain the reference and the outstanding
access or reuse question.

### Current result and immediate next task

The user also authorized publishing the reviewed enrichment with readable
source sections while retaining the existing visual style. The bounded
[8 October public research release](PUBLIC_RESEARCH_RELEASE_2026_10_08.md)
records the explicit public selection, license controls and source/model
qualifications. Original private findings and useful candidates remain saved;
research coverage and public coverage continue to be counted separately.

The user subsequently authorized **broader reading, one mineral at a time**,
with the intention of covering the population and attaching sources to each
useful finding. Checkpoint 96 / run 102 saved Aliettite: **23 source observations
and review notes from three publications, plus a readable private draft**.
Its definition, treatment-dependent layer behaviour, occurrences, specimen
context and qualified geological interpretations now accompany the record.
Unestablished physical/optical measurements remain explicit research gaps.

The priority checklist tracks all **1,993 records without attached COD
relationships**. Before this broader pass, 518 already had publication
observations and 1,475 had none. After Aliettite, those counts are **519 and
1,474**. One record has received this new first pass; 1,992 await it. Prior
partial research and a completed first pass do not establish exhaustive
profile completion. Each new finding retains a source URL/title, exact locator,
specimen/conditions, qualification and review status; captured source bytes
also retain a hash. Private research can advance without a COD assignment.

The following checkpoints describe the earlier resumed COD work.

The user resumed direct record review on **8 October 2026**. Checkpoints 93–95 /
ingestion runs 99–101 saved eight mineral records, adding **6 confirmed mineral
targets, 8 primary COD entries, 156 source observations and 178 evidence rows**.
The latest batch reviewed all five selected entries for Antarcticite,
Aphthitalite and Arcanite and added 76 evidence rows:

| Mineral | Added evidence rows | Saved result |
|---|---:|---|
| Antarcticite | 33 | Two verified laboratory-grown counterparts; separate X-ray and neutron studies |
| Aphthitalite | 26 | Verified synthetic model; historical Glaserite model retained with a sulfate-geometry warning |
| Arcanite | 17 | Confirmed orthorhombic phase; specimen origin unreported |

All useful entries remain attached to the records. Original formulas, prior
evidence, source conditions and model defects are preserved. A confirmed phase
assignment does not establish natural specimen provenance. The earlier five
resumed records—Amicite, Aluminite, Artinite, Aikinite and Aerugite—remain saved
with their qualifications.

Current totals are **694 enriched minerals, 20,957 added evidence rows,
17,578 source observations on 570 minerals, and 65 primary mineral targets /
76 COD entries**. Useful relationships still cover 4,233 minerals; 1,993
remain research gaps. Integrity and preservation checks passed, with zero
conflicting primary targets and an unchanged public catalogue. Read the
[resumed review and next queue](../data/pilots/cod-crystallography-v1/parallel-review-v1/2026-10-06-12h/resumed-2026-10-08/report.md).
Next broader review: Allophane, then Aluminocerite-(CeCa) and Aluminocopiapite.
The COD leads for Adelite (9012798), Allactite (9000164) and Augelite (9000172)
remain saved as a separate queue. The earlier
twelve-hour automation remains paused; all completed reviews are saved.

The following matching and review notes preserve the earlier 6 October work.

The offline Rust matcher has been implemented; its full pass and reproducible
offline replay passed on 6 October 2026. It inventoried all 535,005 COD metadata
rows and all 6,226 mineral records, preserving the original snapshot and source
locators:

- 3,465 minerals have literal exact-name candidates from 12,775 metadata rows.
- 389 additional minerals have exploratory leads; 847 minerals have such
  leads in total, including overlap with exact-name coverage.
- 2,372 minerals have no name link yet and remain research tasks.
- 14,052 metadata rows enter the unreviewed candidate/lead queue; all 520,953
  unlinked rows remain in the source inventory and original snapshot.

The private report is under
`data/pilots/cod-crystallography-v1/candidate-matching-v1/`, in directory
`0b2c6bac83cfd4f1f6fb6d20634eb02d3415320f29f21c5f58d38ebe0cccb69e`.
The [COD matching workflow](COD_CRYSTALLOGRAPHY_PILOT_V1.md#offline-mineral-matching-and-coverage)
documents the launcher and artifacts. These counts describe research leads,
not accepted mineral matches or published crystallographic facts.

Direct publication review on 6 October identified corresponding COD entries
for 24 minerals without an exact-name candidate, including entries with blank
mineral-name fields and older spellings or names. Eighteen were reviewed in the
first two batches and six more in the continuation. These remain identification
proposals pending pinned-CIF review. The
[record review](COD_CRYSTALLOGRAPHY_PILOT_V1.md#direct-record-review--6-october-2026)
records the IDs, revisions, publication evidence and remaining qualifications.
Continue reviewing records directly; the additional research/capture scripts
were removed at the user's request. The original data and useful leads remain.

The subsequent one-off formula comparison used the same saved snapshot and
all catalog formulas, targeting the 2,761 minerals without literal exact-name
candidates. It retained 4,690 proposed links for 600 minerals: 521 have matching
elemental proportions, and 79 have only qualified substitution, missing-H, or
deuterated-counterpart clues. Fifty-two minerals previously recorded without
any research lead gained a chemistry clue; those are not 52 confirmed matches.
The [formula review](COD_CRYSTALLOGRAPHY_PILOT_V1.md#formula-guided-review--6-october-2026)
records the comparison, newly reviewed identities, useful synthetic analogues,
and unresolved cases. No embeddings or additional COD retrieval were needed.

The continuation reviewed 149 mineral cases: all 139 strongest remaining
formula/name/reference cases and ten additional cases under official former
names. It recorded 97 additional provisional mineral associations, 37 synthetic
or isotope counterparts, and 15 related, theoretical, variety or unresolved
cases. At the end of that batch the ledger held primary association proposals
for 115 minerals and 116 COD IDs, including 24 direct publication comparisons.

The second continuation reviewed another **52 mineral cases**, recording 72
individual decisions, including a correction to an earlier publication-year
note. It added **12 provisional mineral associations**, six compared directly
with primary publications: Branchite, Calciocatapleiite, Ferroberaunite,
Kenotobermorite, Lobanovite and Stangersite. It also retained 21 synthetic
counterparts, two deuterated counterparts, two structural counterparts with
origin unresolved, 12 related-structure cases and three unresolved identities.

The cumulative review now covers **222 minerals**, with primary association
proposals for **127 minerals and 129 COD IDs**. Thirty of those minerals have
direct publication comparisons. Other proposals rely on saved metadata and/or
naming authority and still need original-article, sample and pinned-CIF checks.
The [latest continuation review](COD_CRYSTALLOGRAPHY_PILOT_V1.md#record-review-continuation-2--6-october-2026)
records the decisions and remaining questions. All 259 cumulative judgments
were verified against their preserved source responses.

### Findings committed to mineral records — 6 October 2026

The research is now attached to the actual mineral records in
`data/minerals.db`, through the existing audited mineral-review workflow.
**222 records contain 258 COD evidence links** and a readable COD review
summary. Each link retains the COD ID, saved revision, source reference,
relationship, review basis and the remaining qualification.

- **30 minerals / 31 COD IDs:** identified associations supported by direct
  publication comparisons, recorded as primary assignments.
- **97 minerals / 98 COD IDs:** candidate associations supported by metadata
  and/or naming authority, recorded as candidates rather than primary
  assignments.
- **129 other evidence links:** qualified counterparts, related structures
  and unresolved identities, attached to their relevant mineral records.

The database now enforces one primary mineral target per COD ID. The earlier
research ledgers retain their historical proposal states; current assignments
are recorded on the mineral evidence rows. Uramphite and Metauramphite share
a deuterated lead and remain unresolved. Committing the reviewed reference
relationship does not need to wait for validation of every crystal measurement.
The detailed space-group and unit-cell observations remain a separate task.

The commitment receipt, original record snapshots and backup reference are in
`data/pilots/cod-crystallography-v1/record-commit-v1/2026-10-06/`.
The browsing site's exported catalog is a separate release and has not been
replaced by this administrative record update.

### Exact-name links and structure observations committed — 6 October 2026

The next update attached **all 12,775 exact-name COD references to their 3,465
mineral records**. They are labelled `name_matched`, with specimen identity and
crystal measurements unchecked. Three historical labels conflict with an
already reviewed identification and are explicitly `name_conflict`:
Tobermorite/Kenotobermorite (`9002246`), Catapleiite/Calciocatapleiite
(`9004867`), and Beraunite/Ferroberaunite (`9015106`). The reviewed assignments
remain intact; the historical references are retained without a second primary
assignment. Spinel's 646 references and the existing Phenakite image were
preserved in full.

The 30 publication-compared minerals now also contain **31 sourced structure
observations**: all six unit-cell parameters (186 numeric values), space-group
symbols and numbers, and 20 reported temperatures. Values retain their original
lexical form, units, COD revision, archived source locator, publication
references, specimen scope and qualifications. Ideal mineral formulas remain
separate from observed and calculated source formulas.

Four minerals carry explicit qualifications: Krupičkaite and Vyacheslavite have
293 K metadata versus 100 K publication conflicts; Bukovskýite retains both
the COD `c = 10.904 Å` and publication `10.914(2) Å`; Lobanovite retains the
historical, superseded A2 model. Heimite's two apparently repeated datasets
remain separate source entries for one mineral. These observations are saved
in the records; pinned-CIF and model checks remain further work.

A third direct-review continuation inspected and committed **35 additional
mineral cases**: 15 synthetic analogues, 11 composition-compatible structure
counterparts with sample origin unresolved, seven related structures, and two
unresolved identities. None received a new primary assignment.

| Administration result after the first three extensions | Minerals | Mineral–COD links |
|---|---:|---:|
| Exact-name references, identity/structure unchecked | 3,465 | 12,775 |
| Publication-supported identifications | 30 | 31 |
| Metadata/authority candidates | 97 | 98 |
| Counterpart, related or unresolved evidence | Overlapping categories | 164 |
| **All records containing COD evidence** | **3,722** | **13,068** |

There were **13,031 distinct COD IDs** in these relationships. Multiple qualified
references can point to the same source entry; **duplicate primary assignments
remained zero**. Direct record review then covered **257 minerals / 294 judgments**.
All 4,690 formula candidates and original source records remain available;
**348 of the 600 formula-lead minerals still needed a first record review**.
Reviewed-but-unresolved cases remain further research tasks as well.

These three updates used backed-up, atomic evidence extensions with receipts
and entries in the existing ingestion audit tables. They preserved existing
record fields, images, aliases, offers and source evidence. Their private
artifacts are under `record-commit-v1/2026-10-06-name-matches/`,
`record-commit-v1/2026-10-06-structure-values/`, and
`record-review-v1/2026-10-06-continuation-3/`, beneath the COD pilot directory.
The exported browsing catalog remains unchanged.

### Result after direct-review continuation 4

Reviewed and committed **218 name-only gap cases and 124 single-candidate
formula gap cases**, plus the existing Prehnite record. This closes the first
review of 342 additional gap cases. **271 previously unlinked minerals gained
useful COD relationships**; 71 had only misleading hints. All 335 rejected
individual hints remain in the records' review history, and the source data
and original formula candidates remain available.

| Current private-record result | Minerals | Mineral–COD links |
|---|---:|---:|
| Exact-name references, identity/structure unchecked | 3,465 | 12,775 |
| Publication-supported specimen identifications | 39 | 41 |
| Identity candidates | 203 | 242 |
| Counterpart, related or unresolved evidence | Overlapping categories | 361 |
| **All records with useful COD relationships** | **3,993** | **13,419** |

The relationships use **13,326 distinct COD IDs**, with **zero conflicting
primary assignments**. Direct review then covered **600 minerals / 980
historical judgments**. These 600 reviewed minerals are not the same group as
the original 600 formula-lead minerals: they also include name/history cases
and publication comparison specimens.

Eight gap minerals gained identified specimens: Asagiite, Bohuslavite,
Carmeltazite, Ciriottiite, Eveslogite, Ferriprehnite, Gunmaite and Plášilite.
Prehnite gained two identified comparison specimens that COD labels
ferriprehnite. Ferriprehnite retains its identified Fe-dominant specimen and
related links to those comparisons. Structure evidence now includes **41
specimen-scoped observations and 246 unit-cell numbers**, retaining source
formulas, conditions, provenance and qualifications. Identification and
validation of individual structure measurements remain separate: some
publication-table, model and pinned-CIF comparisons are still pending.

| Remaining records without useful COD relationships | Minerals | Next action |
|---|---:|---|
| Multiple formula candidates, first review pending | 224 | Compare polymorph, specimen origin and publication evidence |
| Inspected hints all rejected | 71 | Consult original mineral descriptions and bibliography |
| Neither initial name nor formula lead | 1,938 | Start from original descriptions and structural references |
| **Total** | **2,233** | |

This batch saved 696 evidence claims across 343 material records using a
backed-up atomic transaction and the existing ingestion audit. Integrity,
foreign-key and preservation checks passed; existing record data and evidence
were retained. The exported public catalog remains unchanged. No additional
scraper or embeddings were introduced. The
[review report](../data/pilots/cod-crystallography-v1/record-review-v1/2026-10-06-continuation-4/report.md),
cumulative review ledger, remaining queues and commitment/verification
receipts are under
`data/pilots/cod-crystallography-v1/record-review-v1/2026-10-06-continuation-4/`.

### Current result after direct-review continuation 5

Reviewed **all 86 gap minerals with two formula candidates**, plus the existing
Whewellite record. Saved 173 judgments and three qualified structure
observations as 176 evidence claims across 87 records. **82 previously
unlinked minerals gained useful COD relationships**. Four had only rejected
hints; all 11 rejected individual hints were retained, and original source
data remains available.

| Current private-record result | Minerals | Mineral–COD links |
|---|---:|---:|
| Exact-name references, identity/structure unchecked | 3,465 | 12,775 |
| Publication-supported specimen identifications | 41 | 44 |
| Identity candidates | 206 | 245 |
| Counterpart, related or unresolved evidence | Overlapping categories | 517 |
| **All records with useful COD relationships** | **4,075** | **13,581** |

The relationships use **13,450 distinct COD IDs**. Direct review covers **687
mineral records / 1,153 historical judgments**, with **zero conflicting
primary assignments**. Structure evidence comprises **44 qualified
observations on 41 minerals and 264 unit-cell numbers**. Reference coverage
includes candidates and related structures; those links do not establish
mineral identity. Apparent repeated COD datasets remain explicitly qualified.

Selenopolybasite gained two identified 120 K entries under the historical
Se-rich antimonpearceite name. Whewellite gained a correctly assigned natural
agave raphide entry whose oxygen-deficient COD nominal formula created a false
Formicaite lead. The Formicaite hint was rejected and retained. The Whewellite
observation preserves biogenic origin, source-formula and hydrogen/water model
limitations, with final publisher-PDF and pinned-CIF checks pending.

Other useful results include laboratory counterparts, different polymorphs
and two computational Saddlebackite models. The 76 new counterparts comprise
59 with origin unestablished, 15 explicit laboratory compounds and two DFT
models. Formula identity did not force assignments to Ni2P minerals or to
different organic molecules with the same aggregate element ratios.

| Remaining records without useful COD relationships | Minerals |
|---|---:|
| At least three formula candidates, first review pending | 138 |
| Inspected hints all rejected | 75 |
| Neither initial name nor formula lead | 1,938 |
| **Total** | **2,151** |

The transaction completed as ingestion run 6 with a full backup and
preservation checks. Integrity and foreign-key checks passed. Existing
record identity, ideal formulas, non-COD fields, evidence, images, aliases,
offers and publication/review history were retained. The public catalog and
original COD execution index remain unchanged. The
[record-level review table](../data/pilots/cod-crystallography-v1/record-review-v1/2026-10-06-continuation-5/report.md),
cumulative ledger, remaining queues and commitment/verification receipts are
under `data/pilots/cod-crystallography-v1/record-review-v1/2026-10-06-continuation-5/`.

Use the existing exact-name/authority-alias rules for the COD pilot. Keep
broader matching hints and promising references as exploratory research leads;
they can guide investigation without automatically establishing mineral
identity. An absent candidate is a research gap, not proof of absence.

**Next:** continue direct review of the **138 remaining formula-lead cases**,
starting with the 28 three-candidate minerals, and
check the exact-name references against chemistry, specimen origin and
structure. Inspect selected CIF revisions to resolve the recorded model
or value qualifications and the remaining hydration/site-chemistry questions.
Preserve useful analogues and historical
models alongside mineral evidence. Use reviewed identifications to prepare challenge eligibility
and the frozen selection, then run the existing 96-record pilot. Continue
attaching each completed review to its mineral record. Use the database's
primary-assignment guard for resolved identities and preserve candidate,
counterpart and related links alongside them. Reference identification and
validation of individual crystallographic measurements are separate decisions.
Whole-registry approval, complete catalog coverage, the folder reorganization,
and images do not need
to finish first. Resolve source access and reuse for the material being handled,
and scientific review and public-field readiness for the claims being promoted.
An unresolved issue holds the affected item while independent work continues.

### Active twelve-hour review — 6–7 October 2026

The user authorized three reviewing agents and a coordinator to continue until
**7 October 2026, 08:41:32 Budapest**. They read original publications,
compare each candidate's chemistry and structure, and fill source-linked
details even where no COD assignment is available. Commands retrieve, format
and check evidence; they do not generate scientific judgments. The twelve-hour
window has ended and the scheduled continuation is paused. All completed
reviews are saved in the private administration records.

| Final twelve-hour progress through checkpoint 92 | At dispatch | Final |
|---|---:|---:|
| Mineral records with useful COD relationships | 4,075 | 4,233 |
| Publication-supported identified minerals | 41 | 59 |
| Primary COD assignments | 44 | 68 |
| Formula cases awaiting first review | 138 | 0 |
| Conflicting primary assignments | 0 | 0 |

Saved **685 distinct records and 20,756 evidence entries** in the window. Lithiotantite,
Håleniusite-(La), Iseite, Goldschmidtite, Kaliophilite, Hemleyite and Karwowskiite gained
confirmed natural COD assignments. Calcioveatchite and Alumino-oxy-rossmanite now also have historical natural models assigned, with source geometry and chemical-interpretation limits preserved. Four tourmaline COD entries represent two historical datasets; Abelloemringerite, Achyrophanite,
Ermakovite, Davemaoite and further minerals gained 17,399 source-qualified publication
observations across 561 records. Wadalite, historical Allende Louisfuchsite,
Katayamalite and Serpierite also gained verified natural primary assignments.
Piilonenite-(Nd) now retains its verified original
natural atomic-model supplement with composition and refinement qualifications.
Åsgruvanite-(Ce) and Akasakaite-(Ce) also retain verified natural CIF models;
Bergbauerite and Arzamastsevite retain their published atomic-coordinate tables.
Maohokite preserves competing interpretations of its structure from the original
natural report and subsequent synthesis study. Hokkaidoite gained two verified molecular crystal counterparts;
78 non-target hints remain preserved as rejected evidence.
Laboratory and related structures
remain useful, qualified reference measurements. All original evidence and
unrelated fields were preserved; backup, integrity and foreign-key checks
passed. All 138 formula cases have a saved first review, and all 553 main
publication reviews are committed. Remaining research includes 1,993 registry
minerals without a useful COD relationship and further original-publication
checks on existing name leads. This is not a claim of worldwide COD absence.
The
[final progress table](../data/pilots/cod-crystallography-v1/parallel-review-v1/2026-10-06-12h/final-report.md)
and [pilot progress](COD_CRYSTALLOGRAPHY_PILOT_V1.md#parallel-direct-review--67-october-2026)
record the saved state, sources and remaining tasks. No completed reviews await saving.

## Resume note — 7 October 2026

The overnight work is saved through **checkpoint 92 / ingestion run 98**.
There are no completed scientific reviews waiting to be written. The scheduled
continuation is paused; the reviewing agents have finished. Continue when the
user returns, rather than restarting the finished queues.

Start with a small batch of promising existing name leads and compare their
actual original publications and COD models. There are 3,462 minerals with
name leads; 59 minerals have separately confirmed primary identities. Then
continue targeted original-description research for the 1,993 remaining COD
relationship gaps (1,915 without initial leads and 78 with rejected hints).
Useful publication details and related or synthetic structures should still
be saved when a natural COD identity cannot be established.

The current private database is `data/minerals.db`. Exact completed IDs,
remaining queues, source qualifications and transaction receipts are in
`data/pilots/cod-crystallography-v1/parallel-review-v1/2026-10-06-12h/`.
Read `final-progress.json`, `coordinator-status.json`, `remaining-tasks.md`
and `checkpoint-92/commit-receipt.json` before dispatching more work. Keep the
coordinator as the sole database writer and use the existing managed admin
container. Preserve prior evidence and one primary target per COD entry.

Specific open source questions include the 1991 Alfredopetrovite synthetic
paper and 1992 correction, the tentative 1983 Katayamalite phase with a larger
cell, and recorded occupancy, geometry and source-version conflicts. The
public catalogue is unchanged. All evidence and backups remain available;
existing uncommitted application/source changes should be inspected separately
before any Git commit or reset.

## Handoff — 5 October 2026

This section preserves the earlier handoff. The current result and next task
are recorded above.

**Completed:** the COD metadata scrape and final integrity verification:
1,000 successful requests, 535,005 metadata rows, about 809 MB. The completed
snapshot and exact run evidence are recorded in the
[COD session handoff](COD_CRYSTALLOGRAPHY_PILOT_V1.md#session-handoff--5-october-2026).

**Next session:** implement the offline Rust mineral candidate matcher and
coverage report, then review crosswalks and run the 96-record crystallography
pilot. Reuse the downloaded snapshot; do not repeat the scrape. Public COD
enrichment and ingestion are still pending. The image dispatcher and common
black-background prompt can be resumed separately.

At this handoff the user ended the session. The requested scheduled monitor
was deleted; no automatic continuation was scheduled.

### Pending repository layout

The user requested a future folder reorganization around these exact names:

- `doc/` — project documentation, replacing the current `docs/` directory.
- `code/` — application source, Rust crates, tooling, and tests.
- `web/` — the public web application, assets, and related WebAssembly work.

This is a recorded task for a later session; no files have been moved. Plan the
exact moves and update Cargo paths, Docker bind mounts, launchers, setup and CI
scripts, deployment paths, tests, documentation links, and project instructions
together. Preserve private data, the completed COD snapshot, the image queue,
Git history, and repository ownership. Keep the canonical development page at
`http://127.0.0.1:18965/` working with its current annotation policy.

## Why this is coordinated

The current public records are strongest on identity, nomenclature, authority
context, and one broad identity evidence association. Descriptions,
classification, properties, safety, and occurrences are almost entirely
absent. Reliable localities, physical properties, crystallography, and other
profile facts require additional sources, licensing review, normalization,
conflict handling, and human review. Designing their shared contract together
avoids repeated schema changes and prevents incomplete fields from looking
authoritative. The facts themselves can then arrive in several bounded,
source-owned releases.

The forest map is currently world context only. It must not imply that a
mineral occurs wherever the map is green, and `discovery_country` must never be
treated as an occurrence location.

## Where maps belong

1. **All Minerals:** a small world-context preview is useful visually. Until
   occurrence data exists, it must state that mineral locations are not yet
   included.
2. **Mineral record:** the primary future use. Show reviewed occurrences for
   that mineral, with source, precision, and date visible for every point or
   area.
3. **Catalog exploration:** after coverage is adequate, allow optional country,
   region, locality type, and confidence filters and an aggregate map view.
4. **Private review:** provide curators a map for detecting swapped coordinates,
   impossible country joins, duplicates, and excessive precision before
   publication.

Do not add occurrence markers to the public map until the public snapshot and
review workflow can carry the complete provenance described below.

## Occurrence/locality model

Use a one-to-many `material_occurrences`-style table. Do **not** put one
latitude/longitude pair on `materials`; a mineral can have many reported
localities, and sources can disagree.

Each occurrence should be able to carry:

- stable internal and public occurrence identifiers;
- `material_id` and a source-owned external occurrence/locality identifier;
- locality name, locality type, country code, first-level administrative area,
  and optional smaller administrative areas;
- WGS84 latitude/longitude or reviewed area geometry;
- coordinate precision, uncertainty radius, and whether coordinates were
  copied, calculated, geocoded, or deliberately coarsened;
- sensitivity policy: public, coarsened, or withheld (for protected sites,
  private land, vulnerable deposits, or license restrictions);
- occurrence basis, such as type locality, collected specimen, observed in
  place, mine/deposit record, literature report, or historical report;
- current/historical status and the observation, collection, or publication
  date when known;
- host rock, deposit type, geological formation, paragenesis, and associated
  minerals when the source supports them;
- evidence/source identifier, exact page/row/record locator, retrieval time,
  license, attribution, and changes notice;
- confidence, review status, reviewer, review time, and an explicit note for
  conflicts or inferred values.

Rules for public display:

- never geocode a free-text locality and silently present it as source data;
- never invent missing precision or display more precision than the source;
- show an area or uncertainty circle instead of a point when that is the honest
  representation;
- keep conflicting source claims separate rather than averaging coordinates;
- do not publish sensitive exact coordinates when coarsening or withholding is
  required;
- label occurrence counts as recorded reports, not abundance or probability;
- avoid heat maps or density estimates until collection and reporting bias can
  be explained.

An SQLite RTree index and bounding-box query can be added when the dataset is
large enough to need them. They are implementation details, not substitutes for
the provenance fields above.

## Questionnaire-governed record modules

The questionnaire defines stable keys, scopes, tiers, value shapes, units,
applicability, missing states, evidence thresholds, conflict rules, and public
consumer behavior for the following modules. This backlog remains the
high-level scientific and map plan; the questionnaire is the normative content
contract.

### Identity and history

- IMA number/status and other authority identifiers;
- accepted name, former names, synonyms, and multilingual display names;
- discovery year, discoverer, type locality, naming etymology, and historical
  notes;
- authoritative classification systems and their versions.

### Chemistry and crystallography

- ideal and observed formula variants, substitutions, and end-member series;
- chemical class and compositional notes;
- crystal system, crystal class, space group, unit-cell parameters, and
  structure references;
- polymorphs, polytypes, solid-solution relationships, and related species.

### Physical and optical properties

- color, streak, luster, transparency, habit, tenacity, cleavage, parting, and
  fracture;
- Mohs hardness, density/specific gravity, magnetism, fluorescence, and other
  diagnostic behavior, including value ranges and conditions;
- optical character, refractive indices, birefringence, pleochroism, dispersion,
  and measurement conditions;
- handling, toxicity, radioactivity, dust, and other safety notes supported by
  a cited source.

### Geological context

- formation environment, deposit type, host rock, alteration, paragenesis, and
  associated minerals;
- specimen/locality notes that remain separate from species-wide facts;
- geographic coverage summaries derived only from reviewed occurrences.

### Evidence quality

- a granular evidence claim for each material fact rather than one citation for
  an entire profile;
- source locator, unit, conditions, uncertainty, and conflicting values;
- source authority, license compatibility, retrieval date, and review state;
- explicit `not_yet_researched`, `not_reported_by_reviewed_sources`,
  `not_applicable`, and `withheld` states instead of a literal `unknown`
  placeholder.

## Images are a separate later project

Images should not block the structured-data enrichment. Handle them in a
dedicated media effort covering:

- specimen, crystal, locality, microscopy, and diagram image types;
- original source, creator, license, attribution, source URL, and retrieval
  date;
- alt text, caption, depicted specimen/locality, and whether an image is
  representative or merely illustrative;
- derivatives, dimensions, checksums, content moderation, and removal policy;
- no hot-linking and no assumption that a source page permits image reuse;
- a clear distinction between sourced, user-uploaded, and synthetic media.

## Modular delivery checklist

Apply this checklist to the module or release being worked on. The bounded COD
technical pilot and private research do not wait for every module to be ready.

1. Review the definitions needed for the next structured module. Use the
   already-approved three-question COD contract for its private technical
   pilot now; complete scientific review and whole-registry promotion for
   final questionnaire-v1 contract acceptance.
2. Review the initial source-admission matrix. Before any bounded private
   adapter work, pass the publisher/work/release, rights, stable-identity, and
   field-ownership gates; use the pilot to satisfy snapshot, reproducibility,
   locator, semantics, crosswalk, yield, and review gates.
3. Finalize the claim, resolution, coverage, and relationship schemas needed
   by that module. Finalize occurrence schemas, uncertainty, and
   sensitive-location policy when implementing occurrence publication.
4. Build deterministic pilot adapters that preserve raw values, source
   locators, transformations, and release manifests.
5. Run the bounded module's reproducible development/holdout pilot and meet
   its provenance, accuracy, conflict, and reproducibility gates. A successful
   subset pilot supports that module's next step; final questionnaire-v1
   acceptance requires the full declared contract.
6. Harden accepted adapters for catalog-scale execution and add
   duplicate/conflict checks plus risk-based curator review.
7. Import core and specialist modules as separate source-owned releases rather
   than one all-or-nothing catalog rewrite.
8. Extend the sanitized public snapshot, worker, UI, search/facets, and
   site-tool contracts without exposing private review data.
9. Publish reviewed occurrences before adding record maps, geographic filters,
   or aggregate occurrence views; retain accessible non-map fallbacks.
10. Publish per-module coverage and limitations so missing values or localities
    are not interpreted as absence.
11. Continue private source-linked description drafts during research. Publish
    source-language descriptions from resolved reviewed facts after the
    relevant structured content stabilizes; require a separate translation
    contract before publishing translated editorial prose.
12. Treat the later image pipeline as its own reviewed release.

## Completion criteria

Release useful reviewed subsets as they become ready, with their coverage and
limitations visible. Open questions in other modules do not delay them.

For modules included in a completed release, every eligible core question has
an explicit coverage state, conditional questions have applicability decisions
and the corresponding coverage or inapplicability context, and the module's
declared review and release criteria have passed. Every published value can
answer: “what does this mean, where did it come from, how precise is it, may we
reuse it, and who reviewed it?” Merely assigning `not_yet_researched` records
pending work; it does not establish completed research.

Report overall research progress, module completion, and publication separately.
Missing, inapplicable, withheld, and conflicting states remain visible with
public-safe context. Retain unpublished evidence and unresolved leads for later
releases. The public map must remain useful without color, pointer input, or
exact coordinates, and mineral records must remain usable when the map or
media package is unavailable.
