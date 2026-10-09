# COD crystallography adapter pilot v1

Status: **private pilot; resumed by the user on 8 October 2026 and saved through checkpoint 96 / run 102. Current totals: 694 distinct minerals enriched, 20,957 added evidence rows and 17,578 source-qualified observations on 570 minerals. Useful COD relationships cover 4,233 minerals; 65 publication-supported primary targets / 76 primary COD entries; 2,614 qualified COD structure observations. Broader one-at-a-time publication research has started with Aliettite. All 138 formula cases and all completed reviews are saved; zero conflicting primary assignments. The earlier twelve-hour window is closed and its scheduled continuation remains paused.**

Source decision: **`pilot_only`**

Population: the existing 6,226 valid mineral species in the questionnaire
baseline. This pilot must not add, merge, rename, withdraw, or otherwise change
that population.

Questionnaire:
[`docs/MINERAL_RECORD_QUESTIONNAIRE_V1.md`](MINERAL_RECORD_QUESTIONNAIRE_V1.md)

Source admission decision:
[`docs/MINERAL_SOURCE_ADMISSION_V1.md`](MINERAL_SOURCE_ADMISSION_V1.md)

## Session handoff — 5 October 2026

The metadata retrieval has already run successfully. **Do not restart the
1,000-request scrape for the next stage.** Continue offline from the preserved
snapshot. That handoff left work for the user's next session. The subsequently
authorized 6–7 October parallel review used a same-chat scheduled continuation,
now paused. Its historical results and the 8 October resumed review appear below.

Completed:

- All **1,000 / 1,000** planned COD namespace requests have successful receipts.
- The responses contain **535,005 metadata rows** and total **808,925,630 bytes**
  (about 809 MB). These are COD entries, not 535,005 mineral species or reviewed
  matches to our catalog.
- Retrieval completed at **17:18:18 CEST**; final offline verification passed
  at **17:23:03 CEST**, 5 October 2026 (Europe/Budapest).
- The recovery-and-fetch runner exited with **code 0**, including its final
  `verify-execution` step. Four documented, bounded transport recoveries remain
  in the preserved history.
- Snapshot identity:
  `sha256:26a44255de2b37696480608a0b3419fcd5083bc1d46119d7dc6643e40c0002dd`.

Private evidence, ignored by Git:

- `data/pilots/cod-crystallography-v1/metadata-discovery-execution-index.json`
  records `status: complete`, the receipts, counts, and snapshot identity.
- `data/pilots/cod-crystallography-v1/objects/sha256/` preserves raw responses
  and recovery evidence by content hash.
- `data/pilots/cod-crystallography-v1/runs/metadata-20261005T150849Z/` contains
  the completed run log, code commit, timestamps, and `exit-code.txt`.

Pending, in order:

1. Implement an **offline Rust candidate matcher** against the frozen
   **6,226-record** mineral population. Preserve COD IDs, revisions, source
   locators, and candidate signals; produce coverage and review reports for
   candidates, ambiguous cases, and records without candidates. Formula alone
   must not establish mineral identity.
2. Review and freeze crosswalk decisions. A candidate is not an accepted match.
3. Follow the staged sequence below: freeze challenge eligibility and the
   96-record split, retrieve selected revision-pinned CIFs, and validate the
   adapter on development and holdout records.
4. Only after a passing pilot, implement the reviewed claim-only ingestion
   path and make the separate source-admission and publication decisions.

Separate pending maintenance: reorganize the project around `doc/`, `code/`,
and `web/`, as recorded in the
[repository-layout backlog](MINERAL_RECORD_ENRICHMENT_BACKLOG.md#pending-repository-layout).
This folder migration has not started; the evidence paths above are current.

**No COD crystallographic values have been normalized, ingested, or published
by this retrieval.** The public catalog remains unchanged and COD remains
`pilot_only`. The image dispatcher and black-background image instructions are
ready for a separate continuation; image generation does not block matching.

## Offline candidate report — 6 October 2026

The Rust matcher completed its first full pass over the preserved metadata
snapshot. Every one of the 535,005 source rows is inventoried, including
520,953 rows without a proposed link. Coverage reports include all 6,226
existing minerals. Offline replay reproduced all four artifact hashes and the
manifest exactly. Container validation passed 40 contract tests, 34 Rust
tests, formatting, and Clippy checks.

- 12,775 source rows give literal exact-name candidates for 3,465 minerals.
- 1,277 additional source rows have exploratory leads only. Exploratory leads
  cover 847 minerals overall, of which 389 have no exact-name candidate.
- 2,372 minerals have no name link yet; this is a research gap, not an absence
  claim.
- The unreviewed review queue contains 14,052 source rows. No literal exact
  name mapped to multiple population IDs in this snapshot; ambiguity in
  broader hints and scientific identity still requires investigation.

Current private report directory:
`data/pilots/cod-crystallography-v1/candidate-matching-v1/`, child
`0b2c6bac83cfd4f1f6fb6d20634eb02d3415320f29f21c5f58d38ebe0cccb69e`.

The original snapshot is unchanged. No crosswalks have been accepted, no CIFs
have been retrieved by matching, and no crystallographic values have been
normalized or published. Next: review the proposed links and source evidence,
prepare reviewed challenge eligibility, and freeze the 96-record selection.
The [matching workflow](#offline-mineral-matching-and-coverage) describes the
artifacts and offline verification command.

## Direct record review — 6 October 2026

Review of our frozen records, preserved COD metadata, and published mineral
descriptions identified the following corresponding entries among minerals
previously reported without an exact-name candidate. These are concrete
identification proposals based on source review, not accepted pilot crosswalks
or ingested crystallographic values. Revision-pinned CIF contents have
not yet been inspected for this batch.

| Our mineral | COD ID / saved revision | Identification evidence |
|---|---|---|
| Auerbakhite | `1566214` / `272367` | Blank COD mineral-name field; the [description paper](https://www.jgeosci.org/detail/jgeosci.321/abstract/) matches the title, DOI, formula and reported cell. |
| Bouškaite | `1566236` / `272392` | Blank name field; [the paper](https://www.jgeosci.org/detail/jgeosci.287) matches the DOI, composition and cell. |
| Bavsiite | `1557166` / `247283` | Blank mineral-name field; [the paper](https://doi.org/10.1180/mgm.2019.59) matches the DOI, formula, cell and symmetry. |
| Gladkovskyite | `1566237` / `272393` | Blank name field; [the paper](https://www.jgeosci.org/detail/jgeosci.290) matches the DOI, formula and cell. |
| Babánekite | `1566254` / `272417` | Blank name field; [the description](https://www.jgeosci.org/content/jgeosci.248_plasil.pdf) matches the DOI and cell. Keep the COD refinement formula distinct from the ideal and microprobe formulas. |
| Horákite | `1566251` / `272414` | Blank name field; [the paper](https://www.jgeosci.org/detail/jgeosci.267) matches the DOI, cell and symmetry. Preserve the sample's As/P occupancies separately from the ideal formula. |
| Ježekite | `1566261` / `272424` | Blank name field; [the paper](https://www.jgeosci.org/detail/jgeosci.203) matches the DOI, cell and symmetry. COD's summary formula omits H; do not replace the registry's hydrated formula with it. |
| Sofiite | `9011798` / `291877` | COD uses `Sophiite`; [the structure paper](https://doi.org/10.1180/minmag.1992.056.383.11) matches our reference, chemistry and cell. |
| Mosandrite-(Ce) | `1519938` / `176429` | COD uses `mosandrite`; our registry cites the same 2013 paper and its formula agrees. The [2017 nomenclature paper](https://rruff.info/uploads/MM81_1457.pdf) explicitly records the change to `mosandrite-(Ce)`. |

These findings demonstrate why no exact-name candidate cannot be interpreted
as absence from COD. They do not establish revised coverage for the entire
catalog. The preliminary broader search preserved leads for further direct
review, including related-mineral mentions and chemical analogues; its counts
must not be reported as confirmed matches.

Useful evidence also needs its correct scope:

- Majindeite has a formula-compatible COD lead (`1563738` / `266736`) from a
  study of `T2Mo3O8` compounds. The [nolanite-supergroup paper](https://ejm.copernicus.org/articles/37/133/2025/ejm-37-133-2025.html)
  describes majindeite as a natural analogue of synthetic `Mg2Mo3O8`. Retain
  the lead as analogue research; a natural-mineral determination is not yet
  established.
- Aplowite's lead (`9007885` / `291455`) is explicitly deuterated and synthetic
  in the saved metadata. Keep those qualifiers with the evidence.
- The Berezanskite hit (`9010641` / `283960`) actually identifies **Faizievite**.
  Its title discusses berezanskite structural blocks. Retain it as a related
  structure reference, not a Berezanskite determination.

The user requested direct review rather than another scraper or script. The
additional research/capture scripts and their launcher commands were removed;
the preserved metadata, earlier matcher and useful private research evidence
remain. Continue direct review of unresolved records and their references.

## Formula-guided review — 6 October 2026

The user requested chemical comparison before considering embeddings. A
one-off inspection inside the existing admin container compared all 535,005
saved COD summaries against the catalog formulas, targeting the 2,761 minerals
without literal exact-name candidates. No new scraper, project command,
embedding pipeline, or database mutation was added.

Original formulas are retained. The comparison expands brackets and hydrate
components, removes explicit positive oxidation-state notation, and compares
exact rational atom proportions independent of element order or formula-unit
scale. Vacancies and zero occupancies remain diagnostic context. For example,
`Mg2Mo4+3O8` becomes `Mg2 Mo3 O8`, while `(Ta2/3Mn2+1/3)O2` has the proportions
`Mn Ta2 O6`. Hydrogen omission, deuteration, and elemental substitution-site
compatibility remain distinct qualified clues. A compatible mixed-site formula
does not establish dominant elements, structural ordering or mineral identity.

Across all 6,226 minerals, 5,350 formulas have fixed atom compositions and 389
have comparable elemental substitution sites. The remaining 487 formulas are
retained with unresolved notation, ranges, compound substitutions or malformed
brackets. Of the 2,761 target minerals, those counts are 2,305, 189 and 267.
The pass parsed 534,401 COD formulas; the 604 unresolved source rows remain in
the preserved snapshot. These counts describe this comparison's capabilities,
not judgments that unresolved formulas or records lack value.

| Best formula clue per target mineral | Minerals |
|---|---:|
| Same elemental proportions | 521 |
| Compatible variable substitution sites | 53 |
| Matching non-H proportions; source omits hydrogen | 18 |
| Deuterated counterpart only | 8 |
| **Total with formula clues** | **600** |

The categories above do not overlap. The 4,690 candidate pairs include new
pairs for 127 minerals compared with the previous equal-formula pass; 52
minerals previously recorded with no research lead gained a chemistry clue.
These are not confirmed assignments. Broad compatibility can identify related
species, synthetic counterparts, or misleading coincidences. In particular,
removing hydrogen from an organic formula can leave only carbon; such bare
element comparisons are retained as uninformative clues rather than useful
species identifications.

Publication review supports nine additional mineral-identification proposals,
bringing the documented total to 18 minerals and 19 COD records. None is an
accepted pilot crosswalk; pinned CIFs have not yet been inspected.

| Our mineral | COD ID / saved revision | Identification evidence and qualification |
|---|---|---|
| Heimite | `1571002`, `1572359` / `295836` | The [description](https://ejm.copernicus.org/articles/36/153/2024/) matches the DOI, locality, composition and cell within quoted uncertainty. Both IDs appear to describe the same dataset; retain both and count one mineral identification. |
| Richardsite | `1558193` / `252944` | Blank mineral-name field; the [description](https://www.mdpi.com/2075-163X/10/5/467) matches the DOI, `Zn2CuGaS4`, cell and space-group number 121. |
| Riesite | `1557789` / `250752` | Our `TiTiO4` and COD's `TiO2` have the same proportions. The [description](https://www.mdpi.com/2075-163X/10/1/78), cell and space-group number 13 distinguish riesite from other `TiO2` minerals. |
| Eliopoulosite | `1557744` / `250387` | The [description](https://www.mdpi.com/2075-163X/10/3/245) matches the lowercase label, DOI, `V7S8`, cell and symmetry. An earlier entry, `1557233`, instead cites a grammatikopoulosite paper; retain that publication mismatch separately. |
| Vandermeerscheite | `1566238` / `272394` | Blank name field; the [description](https://www.jgeosci.org/detail/jgeosci.288), DOI, cell and symmetry agree. COD omits H; retain the mineral's hydrated formula. |
| Alumoåkermanite | `9017542` / `292100` | The [description](https://www.cambridge.org/core/journals/mineralogical-magazine/article/abs/alumoakermanite-cana2almgfe2si2o7-a-new-mineral-from-the-active-carbonatitenephelinitephonolite-volcano-oldoinyo-lengai-northern-tanzania/467E93B3ED56275968523132099DE241), locality, reference and cell agree. Observed Ca/Na and Al/Mg/Fe totals satisfy the two substitution sites. |
| Hodgesmithite | `2108521` / `227861` | The [description](https://onlinelibrary.wiley.com/doi/full/10.1107/S205252061901343X), DOI, cell and `P3` agree. Keep COD's nominal and calculated formulas distinct from the microprobe composition. |
| Siidraite | `1561470` / `263553` | The [2016 unnamed natural phase](https://www.sciencedirect.com/science/article/abs/pii/S0022459616300792) matches our second reference, formula and cell. The [2017 naming paper](https://publications.diamond.ac.uk/pubman/viewpublication?publicationId=8259) identifies siidraite on the same NHM specimen, BM84642. |
| Krupičkaite | `1566215` / `272368` | The [description's Table 3](https://www.jgeosci.org/content/jgeosci.318_Steciuk.pdf) explains the different cell and `P21/n` as a low-temperature electron-diffraction structure. The paper reports 100 K while COD says 293 K; retain that unresolved conflict and the partial-H/refined-water composition. |

Three more reviewed leads are useful **synthetic analogues**: Griffinite
(`2206495` / `176774`), Verneite (`1000236` / `130149`), and Fluoro-tremolite
(`9000375` / `291269`). The corresponding natural-mineral descriptions or
structure paper explicitly identify the laboratory counterpart. Preserve this
evidence with its sample origin. Fluoro-tremolite previously had no research
lead. Formula-compatible records identified as normandite remain related
references for Låvenite, rather than Låvenite assignments.

Private results are under
`data/pilots/cod-crystallography-v1/formula-research-v1/2026-10-06/`:
`report.md`, `formula-candidates.jsonl`, `mineral-formula-review.jsonl`,
`record-review.jsonl`, `analysis-summary.json`, `priority-candidates.json`, and
`research-manifest.json`. Every candidate retains a COD ID, revision, original
formula and hashed metadata locator. The review ledger records identity,
analogue and non-identity judgments separately. Supported proposals are unique
by COD ID in that ledger, but remain inactive. An enforced final assignment
register is not yet implemented; it must permit at most one active mineral
target per source determination and preserve decision history. Ambiguous
candidate lists may contain several mineral targets without assigning any.

## Record review continuation — 6 October 2026

This continuation inspected 149 mineral cases: the 139 strongest remaining
formula candidates with name/title/reference evidence, plus ten cases found
under official former names. The private review records 163 individual COD
judgments, including displaced choices and additional historical datasets.

| Best outcome for each mineral in this batch | Minerals |
|---|---:|
| Provisional mineral association | 97 |
| Synthetic structural counterpart | 34 |
| Deuterated counterpart | 3 |
| Related structure or different hydrate | 7 |
| Theoretical structure only | 2 |
| Identity still unresolved | 7 |
| Specific variety/sample relation | 1 |
| **Reviewed cases** | **149** |

The cumulative ledger contains primary association proposals for **115
minerals and 116 COD IDs**, including the two retained Heimite records. Only
**24 minerals** have direct primary-publication comparisons so far. The other
associations are supported at the saved-metadata or naming-authority level;
they are not full article/CIF validations. Sample origin remains unspecified
where it cannot be established. Forty minerals have useful synthetic or isotope
counterparts in the cumulative review, kept separately from primary mineral
associations. No proposal is accepted or active.

Six additional direct publication comparisons:

| Mineral | COD ID / saved revision | Evidence and remaining qualification |
|---|---|---|
| Balićžunićite | `9017701` / `292100` | Natural La Fossa sample; formula and all cell parameters match the [primary abstract](https://doi.org/10.1180/minmag.2015.079.3.06). |
| Bukovskýite | `9014967` / `283658` | Description reference, locality and formula agree. COD c=10.904 differs from 10.914(2) in the [primary abstract](https://www.jstage.jst.go.jp/article/jmps/107/3/107_110930/_article); preserve the discrepancy. |
| Molybdophyllite | `9014752` / `201816` | Formula, C2 and cell match the monoclinic polytype in the [primary abstract](https://doi.org/10.1180/minmag.2012.076.3.04). |
| Delchiaroite | `1577029` / `305453` | Natural La Piana sample, formula, Pmmn and cell match the [2026 description](https://ejm.copernicus.org/articles/38/153/2026/). |
| Hibbingite | `8104545` / `250400` | Natural Norilsk sample, Pnma and cell match the [primary abstract](https://doi.org/10.1515/zkri-2018-2124). |
| Vyacheslavite | `7232933` / `216509` | The [primary article](https://pmc.ncbi.nlm.nih.gov/articles/PMC9065332/) combines natural-crystal PEDT with DFT. Preserve the 100 K paper versus 293 K COD conflict and determine the specific CIF model. |

The [2022 CNMNC naming notice](https://ejm.copernicus.org/articles/34/463/2022/)
explains specific former names, including alpha/beta uranophane, Nováčekite
I/II, Andorite IV/VI, beta fergusonite, Ice VII and beta sulfur. These phase and
hydration qualifiers must stay attached to the aliases. The
[2013 guidelines](https://cnmnc.units.it/cnmnc_booklet/2013%20Hatert%20et%20al_%20guidelines%20suffixes-prefixes%20historical%20name.pdf)
explain natroapophyllite to fluorapophyllite-(Na), and the
[2025 notice](https://ejm.copernicus.org/articles/37/75/2025/) explains
svornostite to svornostite-(K). These are research associations; the catalog
names and formulas have not been edited.

Specific cleanup findings:

- The original autunite-sheet and zippeite-group papers identify synthetic
  crystals even where the COD origin field is blank. Hydronováčekite,
  Nováčekite, cobaltzippeite, natrozippeite and zinczippeite are useful
  laboratory counterparts in the selected datasets.
- Synthetic M-prime YTaO4, COD `9011071`, is a published structural analogue
  of iwashiroite-(Y), although its saved mineral label says Formanite-(Y).
- For Fowlerite, COD `9003691` preserves sample 15-4041, explicitly called
  fowlerite in the [primary paper](https://www.rruff.net/doclib/am/vol90/AM90_969.pdf).
  Its cell agrees. Retain a variety/sample relation to rhodonite; a shared
  paper title does not establish that every rhodonite sample is fowlerite.
- The Symplesite candidates identify parasymplesite; the Brockite candidate
  identifies rhabdophane-(Ce). Earlandite's candidate is explicitly a different
  synthetic polymorph. Preserve these relations without forcing identities.
- Metauranocircite's former-name record is a synthetic seven-water compound;
  the catalog formula has six waters. Mertieite and paralomonosovite also have
  unresolved source/catalog composition differences. Keep the evidence and
  the questions together.

Private artifacts are in
`data/pilots/cod-crystallography-v1/record-review-v1/2026-10-06-continuation/`:
`report.md`, `reviewed-records.jsonl`, `mineral-review-status.jsonl`,
`cumulative-review-ledger.jsonl`, `proposed-primary-associations.jsonl`,
`specific-review-issues.jsonl`, `remaining-formula-review.jsonl`, and the
hashed `research-manifest.json`. Every reviewed record retains its original
metadata locator and revision. The proposed-primary index checks one target
per COD ID and preserves review history; it is not a database assignment
register. Variety relations and analogues do not reserve a second primary
mineral target.

All 4,690 formula proposals remain retained. Of the 600 mineral cases with
formula leads, 435 still need a first record review. Continue with those cases
and the selected revision-pinned CIFs. No persistent research script, embedding
pipeline, or additional COD retrieval was added in this continuation.

## Record review continuation 2 — 6 October 2026

This batch reviewed **52 additional mineral cases**, recording **72 COD
judgments**, including an appended correction to an earlier review note.

| Best outcome per mineral in this batch | Minerals |
|---|---:|
| Provisional mineral association | 12 |
| Synthetic structural counterpart | 21 |
| Deuterated counterpart | 2 |
| Structural counterpart; sample origin unresolved | 2 |
| Related structure or different mineral | 12 |
| Identity still unresolved | 3 |
| **Reviewed cases** | **52** |

Six additional primary-publication comparisons identify specific historical
datasets:

| Mineral | COD ID / saved revision | Evidence and qualification |
|---|---|---|
| Branchite | `9016769` / `292100` | The [renaming paper](https://www.cambridge.org/core/journals/mineralogical-magazine/article/hartite-renamed-branchite/39015ED92557ED31925A5D3278EA0F94) identifies the 1998 Bílina Hartite dataset and reproduces its P1 cell. |
| Calciocatapleiite | `9004867` / `303136` | The [original paper](https://www.rruff.net/doclib/cm/vol42/CM42_1037.pdf) identifies natural calcium-dominant catapleiite and the saved Pbnn cell. |
| Ferroberaunite | `9015106` / `303136` | The [description](https://www.cambridge.org/core/journals/mineralogical-magazine/article/ferroberaunite-fe2fe35po44oh56h2o-a-mixedvalence-iron-member-of-the-beraunite-series-from-the-gravel-hill-mine-perranzabuloe-cornwall-england/D8C3CD0261018C56C07D34A8E677FAA7) reidentifies the 1992 Mullica Hill Beraunite sample as mixed-valence ferroberaunite. The old red Giessen model is retained separately. |
| Kenotobermorite | `9002246` / `283658` | The [later polytype comparison, Table 6](https://pureportal.spbu.ru/files/99728370/2022_Paratobermorite.pdf) assigns the exact historical B11m cell to kenotobermorite-2M. Keep the order-disorder/polytype context. |
| Lobanovite | `9013161` / `287555` | The [description manuscript](https://iris.unito.it/retrieve/handle/2318/1557068/143177/1261-sokolova_1455976949670.pdf) identifies Shi et al.'s 1998 sample 3086 as lobanovite. Its A2 refinement was superseded; identification does not approve the historical symmetry/site model. |
| Stangersite | `9005509` / `291351` | The [description, sections 6–7 and Table 7](https://www.jgeosci.org/content/jgeosci.306_Sejkora.pdf) names the 2001 natural SnGeS3 dataset and reproduces its single-crystal cell, separately from powder and synthetic measurements. |

Six further associations rely on inspected metadata and/or official naming
definitions: Gismondine-Ca, Gerhardtite, Sørensenite, Columbite-(Mn),
Bixbyite-(Fe) and Bixbyite-(Mn). The Fe/Mn suffixes use each row's composition;
a paper title listing several specimens cannot identify every row. The
additional Pcab bixbyite-(Mn) dataset is retained under the current definition.
Unreported sample origin remains unknown.

Useful cleanup results:

- Anthracene/Freitalite and phenanthrene/Ravatite share `C14H10` but are
  different molecules. Their records remain useful with the correct relation.
- The Pnnm CuSe2 record labelled Krutaite is a synthetic Petříčekite
  counterpart; cubic Pa-3 CuSe2 is the Krut'aite structure. The DFT model is
  retained separately from experimental datasets.
- Synthetic M-prime YTaO4 remains an Iwashiroite-(Y) counterpart. The
  Takanawaite-(Y) lead is a different M-type structure and is not established
  by that record.
- Elpidite versus Hydroterskite illustrates water/hydroxyl and topology
  differences hidden by atom totals. The sulfite dimorphs, Millsite/Teineite,
  and Goslarite/Zincmelanterite also require their structural distinctions.
- Uramphite and Metauramphite share normalized trihydrate proportions and a
  deuterated candidate. The lead remains unresolved for both. Another row's
  fully deuterated title conflicts with its mixed H/D formula; preserve that
  source discrepancy.
- The earlier Achávalite review note said 1932, while the saved publication
  year is 1925. The correction is appended to the ledger, preserving history.

The cumulative review covers **222 minerals** and **259 record judgments**.
Primary association proposals cover **127 minerals and 129 COD IDs**; **30
minerals** have direct primary-publication comparisons. All proposals remain
inactive and require the relevant article/sample/CIF validation. The register
checks one primary target per COD ID; shared analogue and unresolved links do
not create competing primary assignments. Final database enforcement remains
pending.

Private artifacts are in
`data/pilots/cod-crystallography-v1/record-review-v1/2026-10-06-continuation-2/`:
`report.md`, `manual-review-decisions.json`, `reviewed-records.jsonl`,
`mineral-review-status.jsonl`, `cumulative-review-ledger.jsonl`,
`proposed-primary-associations.jsonl`, `specific-review-issues.jsonl`,
`remaining-formula-review.jsonl`, `review-summary.json`, `verification.json`
and `research-manifest.json`. The JSON decision sheet records the manual
judgments; no persistent research script or new scraper was added.

All **259 judgments** were verified against their original hashed responses:
28 objects and 326,428,911 bytes. Previous review artifacts, the source
execution index and frozen catalog retain their hashes. All **4,690 formula
candidates** remain available; **383 of the 600 formula-lead cases** still need
a first record review. Continue those reviews and resolve the selected
pinned-CIF model questions independently.

## Reviewed evidence committed to mineral records — 6 October 2026

Following the user's direction to commit useful findings into the records,
the reviewed relationships are now stored on **222 actual mineral records** in
`data/minerals.db`. The existing import/review workflow recorded and approved
the exact record updates, preserving their previous identity, description,
verification state, official evidence and other existing fields.

Each mineral now has a `cod_records` collection, a readable
`cod_review_summary`, and source evidence under `properties.cod_record_link`.
The **258 links** carry their COD ID, saved revision, citation, relationship,
evidence level, sample origin, qualifications and review note:

| Recorded relationship status | Links |
|---|---:|
| Identified from primary-publication comparison | 31 |
| Candidate from metadata and/or naming authority | 98 |
| Qualified structural counterpart | 84 |
| Related mineral or structure | 34 |
| Identity unresolved | 11 |
| **Total attached evidence links** | **258** |

The 31 identified links cover **30 minerals**, with two retained Heimite COD
records. They are primary source associations now. The 98 candidate links
cover **97 minerals**, including two Bixbyite-(Mn) datasets; they preserve
their uncertainties without reserving a primary assignment. The record label
is more specific than the legacy numeric confidence field, which retains its
default value and is not a calibrated probability.

`idx_cod_record_one_primary_target` enforces one primary mineral target per
COD ID across these evidence rows. It permits multiple candidate/related
links, so shared evidence is retained. A database-copy check rejected a
conflicting primary assignment and allowed non-primary relations.

Reference identification is committed independently of detailed crystal
measurements. No unit-cell or space-group observation is asserted by this
record update; those require validation of the specific revision and model.
The older immutable research snapshots retain their proposal states. Current
reference assignments are the database evidence rows, while the broader COD
adapter remains a private pilot. The exported browsing catalog is a separate
release and has not been replaced here.

The commitment artifacts are under
`data/pilots/cod-crystallography-v1/record-commit-v1/2026-10-06/`:
`record-updates.json`, `before-records.json`, `preparation-summary.json`,
`stage-receipt.json`, `commit-receipt.json`, and `report.md`. The receipt
identifies the database backup, approved review IDs and verification results.
Continue writing completed reviews into the records instead of accumulating
only side reports.

## Exact-name references and structure observations committed — 6 October 2026

Following the record commitment above, the user's next instruction authorized
attaching the saved exact-name references, recording the publication-linked
crystal observations, and continuing direct formula-lead review.

| Committed extension | Mineral records | COD references/observations |
|---|---:|---:|
| Literal-name references, identity and structure unchecked | 3,465 | 12,775 links |
| Publication-linked space-group and six-parameter cells | 30 | 31 structure observations |
| Further direct formula-lead review | 35 | 35 links |

The exact-name extension retains every source entry, including 646 for Spinel.
Three source labels conflict with an already reviewed identification:
`9002246` Tobermorite/Kenotobermorite, `9004867`
Catapleiite/Calciocatapleiite, and `9015106` Beraunite/Ferroberaunite.
These links are `name_conflict`; the other literal-name references are
`name_matched`. All are non-primary. Name agreement attaches a useful reference
without asserting specimen identity or validated crystal properties.

The 31 `properties.cod_structure_observation` claims contain **186 unit-cell
values**, space-group symbols/numbers, and **20 reported temperatures**, with
units, raw lexical values, COD IDs/revisions, archived metadata locators and
publication references. They describe source specimens and models. Missing
temperatures and pressures remain explicitly unreported. The existing ideal
formulas were preserved alongside the separate nominal/calculated COD formulas.
The raw metadata was checked against all nine corresponding archived responses;
the observations are extracted from metadata, not from newly read CIF files.

Krupičkaite and Vyacheslavite retain the 293 K metadata versus 100 K publication
conflicts. Bukovskýite retains COD `c = 10.904 Å` and publication
`10.914(2) Å` separately. Lobanovite's A2 model is explicitly historical and
superseded. Kenotobermorite retains its 2M order-disorder context, and Heimite's
two COD entries retain an apparent repeated-dataset relationship. Saving these
qualified observations does not require assigning one universal value to the
species or resolving every CIF/model question first.

The next 35 single-candidate formula cases were manually inspected. Their
recorded outcomes are 15 synthetic analogues, 11 structure counterparts with
origin unresolved, seven related structures, and two unresolved identities.
Publication comparison distinguishes the synthetic hexagonal ebnerite lead
from the related monoclinic epiebnerite mineral, and distinguishes the ideal
PtBi model from the approved driekopite determination. No new primary
identification was assigned by this batch.

At the end of these extensions, administration totals were **3,722 mineral records, 13,068 mineral–COD
relationships and 13,031 distinct COD IDs**. The 31 primary associations then
covered 30 publication-compared minerals. The unique primary-assignment guard
continues to reject conflicting primary targets; non-primary reference sharing
is allowed. Direct review then covered **257 minerals / 294 judgments**, with
**348 formula-lead minerals awaiting a first review**. The two unresolved and
other qualified cases also retain their stated further-research questions.

The extensions used atomic SQLite transactions with a full database backup,
complete before-record snapshots, payload hashes, ingestion audit entries and
commitment receipts. They preserved previous source evidence, record identity,
images, aliases, offers and existing publication/review events. No approval
events were fabricated for these bulk evidence extensions. Integrity and
foreign-key checks passed. The frozen public catalog and original COD snapshot
remain unchanged.

Private artifacts below `data/pilots/cod-crystallography-v1/`:

- `record-commit-v1/2026-10-06-name-matches/`;
- `record-commit-v1/2026-10-06-structure-values/`;
- `record-review-v1/2026-10-06-continuation-3/`.

## Direct record review continuation 4 — 6 October 2026

The user's instruction to proceed authorized direct inspection and commitment
of the remaining existing leads. This batch reviewed **all 218 name-only gap
cases and 124 single-candidate formula gap cases**, plus the existing Prehnite
record. These are 342 additional gap cases and 343 updated mineral records.
The reviews used the saved COD snapshot and original publications. No new
scraper or embeddings were introduced.

| Private-record result after continuation 4 | Mineral records | Mineral–COD relationships |
|---|---:|---:|
| Exact-name references, identity/structure unchecked | 3,465 | 12,775 |
| Publication-supported specimen identifications | 39 | 41 |
| Identity candidates | 203 | 242 |
| Counterpart, related or unresolved evidence | Overlapping categories | 361 |
| **All records with useful COD relationships** | **3,993** | **13,419** |

The relationships use **13,326 distinct COD IDs**. The batch saved 686
individual judgments: ten identified, 144 candidate, 77 counterpart, 77
related, 43 unresolved and 335 rejected identity hints. Rejected hints remain
in the records' review evidence and do not count as useful COD relationships.
No original source record or formula candidate was deleted. **271 previously
unlinked minerals gained useful relationships**; 71 had only rejected hints.

Eight gap minerals gained publication-supported specimen identifications:
Asagiite, Bohuslavite, Carmeltazite, Ciriottiite, Eveslogite, Ferriprehnite,
Gunmaite and Plášilite. The existing Prehnite record gained two identified
comparison specimens. COD entries `1564253` and `1564254` are labelled
ferriprehnite in COD but are prehnite specimens in the original paper; their
primary assignments now belong to Prehnite. Ferriprehnite retains related
comparison links and the identified Fe-dominant entry `1564252`. The unique
primary-assignment guard remains active; **conflicting primary assignments
remain zero**.

Ten additional specimen-scoped structure observations bring the total to
**41 observations on 39 minerals and 246 unit-cell numbers**. Source formulas,
conditions, revision/response provenance and model qualifications are retained.
Bohuslavite's identification uses the publisher's original abstract, with full
model review pending. The two Prehnite specimens are identified from labels
and site chemistry; numerical comparison of the publication cell table remains
pending. Asagiite retains COD 298 K versus publication 293 K; Plášilite retains
COD 293 K versus publication 298(2) K. Carmeltazite's COD O12 title typo is
preserved alongside the publication's O11 correction. These identifications
do not assert that every associated model or numerical observation is validated.

Cumulative direct review then covered **600 mineral records / 980 historical
judgments**. There are **2,233 minerals without a useful COD relationship**:
224 multiple-candidate formula cases awaiting a first review, 71 cases with
only rejected inspected hints, and 1,938 with neither initial name nor formula
leads. Already-reviewed candidate and unresolved cases also retain their
further research questions. An absent useful lead does not establish absence
from COD.

The atomic transaction saved 696 evidence claims across 343 material records,
with a full backup, before-record snapshots, payload hash, ingestion run 5 and
commitment receipt. Integrity and foreign-key checks passed. Previous identity,
non-COD fields, source evidence, images, aliases, offers and publication/review
history were preserved. The frozen 6,226-mineral public catalog matches its
unchanged database hash, and the original COD execution index is unchanged.
The 41 archived response bodies used in this batch were checked before commit.

Private artifacts are under
`data/pilots/cod-crystallography-v1/record-review-v1/2026-10-06-continuation-4/`.
The [review report](../data/pilots/cod-crystallography-v1/record-review-v1/2026-10-06-continuation-4/report.md)
contains the specimen table and primary references. The cumulative review
ledger, 224-case remaining formula queue, 2,233-case gap list, commitment
receipt and final verification are saved alongside it.

At this point **224 multiple-candidate formula cases** still awaited a first
review. Source observations, specimen identification, CIF/model validation
and public-catalog release remain separately recorded decisions.

## Direct record review continuation 5 — 6 October 2026

The next batch reviewed **all 86 gap minerals with exactly two formula
candidates**, plus the existing Whewellite record. All results were committed
directly: 173 judgments and three qualified structure observations, saved as
176 evidence claims across 87 mineral records. **82 previously unlinked
minerals gained useful COD relationships**. Four had only rejected hints:
Daubréeite, Fuchunite, Jimkrieghite and Manganarsite. All 11 rejected individual
identity hints remain in the review evidence; source data was preserved.

| Current private-record result | Mineral records | Mineral–COD relationships |
|---|---:|---:|
| Exact-name references, identity/structure unchecked | 3,465 | 12,775 |
| Publication-supported specimen identifications | 41 | 44 |
| Identity candidates | 206 | 245 |
| Counterpart, related or unresolved evidence | Overlapping categories | 517 |
| **All records with useful COD relationships** | **4,075** | **13,581** |

The relationships use **13,450 distinct COD IDs**. Direct review covers **687
mineral records / 1,153 historical judgments**. There are **44 specimen/model
observations on 41 minerals, containing 264 unit-cell numbers**. The unique
primary-assignment guard remains active, with **zero conflicting assignments**.
Distinct primary COD IDs can still describe apparent repeated datasets; these
are explicitly qualified rather than counted as independent measurements.

Selenopolybasite gained two identified entries: `2100629` / revision `277836`
and `9011318` / revision `293658`. The original mineral description connects
the natural De Lamar type specimen with the older Se-rich antimonpearceite
study. The original 2006 Table 2, compared through indexed article text,
matches the 120 K composition and P21/c cell. Both entries retain the
low-temperature ordered-phase scope and apparent repeated-dataset relationship;
the room-temperature trigonal model remains separate.

Whewellite gained identified entry `8000051` / revision `208205`. The
author-uploaded original proof describes a natural agave calcium-oxalate
monohydrate raphide and the same P21/a cell. Its COD nominal formula omits
oxygen and creates a false Formicaite hint; that hint was rejected on
Formicaite while the specimen was assigned to Whewellite. Nominal/calculated
formulas, biogenic origin, hydrogen/water model limitations and relatively high
residuals are retained. Final publisher-PDF and pinned-CIF comparison remain
pending. The proof's −73.2 °C and its explicitly normalized 199.95 K are stored
alongside COD 200 K; the conversion is not a separate measurement.

The other judgments are three candidates, 76 counterparts, 51 related
structures and 29 unresolved leads. The 76 counterparts comprise 59 with
sample origin unestablished, 15 explicit laboratory compounds and two DFT
Saddlebackite models. Raw 0 K conditions on the DFT entries are calculation
context, not experimental specimen temperature. Lianbinite retains a useful
ammonium glycolate/glycolic-acid laboratory counterpart; a paper title about
pharmaceutical cocrystals did not justify discarding that structure.
Niasite/Johanngeorgenstadtite, Ellinaite and Cadvanite reviews distinguish
same-composition polymorphs. The shared Ni2P leads for Orishchinite and
Transjordanite remain unresolved rather than assigned by formula alone.

**138 formula-lead minerals await a first review**, each with at least three
candidates; the next group has 28 three-candidate cases. **2,151 minerals
remain without useful COD relationships**: these 138, 75 cases with only
rejected inspected hints, and 1,938 with neither initial name nor formula lead.
Already-reviewed candidates and unresolved links also retain further work.

Ingestion run 6 used a full backup, before-record snapshots, payload hash and
atomic commit. Integrity and foreign-key checks passed. Existing mineral
identity, ideal formulas, non-COD fields, source evidence, images, aliases,
offers and publication/review history were preserved. The frozen public
catalog and original COD metadata execution index are unchanged. All 37
archived response bodies used by this batch were checked before commitment.
No scraper, embeddings or new persistent program were added.

The [review report](../data/pilots/cod-crystallography-v1/record-review-v1/2026-10-06-continuation-5/report.md)
contains all 87 record-level outcomes and primary references. The cumulative
ledger, remaining queues, commitment receipt and final verification are saved
under `data/pilots/cod-crystallography-v1/record-review-v1/2026-10-06-continuation-5/`.
Continue direct review with the **138 remaining first-review cases** and save
each useful finding in its mineral record.

## Resumed direct review — 8 October 2026

Checkpoint 93 / run 99 saved three manually compared name leads directly into
the private records. Amicite gained two primary COD entries, explicitly
duplicates of one natural experiment. Aluminite and Artinite gained one each:
actual composition and model comparisons, supported by later original studies,
resolve the phase targets while the older specimens' origins remain unreported.
Later localities and temperatures are not transferred to those older models.

The batch added 50 evidence rows, 44 individual source observations and four
complete COD models. Water/H and angle conflicts, disordered occupancies and
imported displacement defects remain preserved. This batch strengthened
existing relationships; the 1,993 relationship gaps remain.

Checkpoint 94 / run 100 saved Aikinite (9007524 and 9008200) and Aerugite
(9007677), adding 52 evidence rows, 46 source observations and three complete
COD models. The two Aikinite studies share Pnma phase identity with small
cell/coordinate differences and symmetry-equivalent relabelled sites. The
original 1971 first page confirms natural Beresovsk crystals, Harvard 82490,
and reports measured density separately from calculated density. The altered
Pb/Cu displacement values and invalid deposited Pb tensor remain qualified.
Aerugite's Ni8.499 is rounded 5/6 site occupancy; As5+ is a charge annotation.
Its rhombohedral Z1 and conventional hexagonal Z3 cells are equivalent.
The 2022 primary study adds four-spot chemistry and Raman details without
transferring its museum provenance to the 1989 CIF. Mining-fire formation
remains a hypothesis. At checkpoint 94, primary coverage was 64 minerals /
75 COD entries, and 3,457 minerals retained name-only leads. Those two checkpoints total five newly
confirmed minerals, seven COD entries, 102 evidence rows and 90 observations.

Checkpoint 95 / run 101 saved Antarcticite (1001770, 9007715), Aphthitalite
(1011019, 9007639) and Arcanite (9007569), adding 76 evidence rows, 66 source
observations and five complete COD models. Both Antarcticite studies used
laboratory-grown CaCl2·6H2O crystals. Their source coordinates, hydrogen models
and the 1986 deposited a/b discrepancy remain separate. Aphthitalite's 1980
K3Na(SO4)2 model is verified synthetic; its 1928 Glaserite model is retained
with a deposited sulfate-geometry warning, pending the original article.
Arcanite's actual Pnam model agrees with the original 1972 cell and S–O
geometry; its phase is confirmed while specimen origin remains unreported.
Preparation temperatures, diffraction conditions, different compounds in
the same paper and hypothetical force-model charges remain correctly scoped.

Current primary coverage is 65 minerals / 76 COD entries; 3,454 minerals
retain name-only leads. Through checkpoint 95, the resumed session saved eight records, twelve
complete COD models, 178 evidence rows and 156 source observations. Six new
primary mineral targets and eight primary COD entries were added. The 1,993
relationship gaps remain research tasks; enrichment is partial record progress.

The user then authorized broader publication reading, one mineral at a time,
with source attribution and intention to cover the population. Checkpoint 96 /
run 102 saved Aliettite with 23 source observations and review notes from three
publications, plus one private editorial draft tied to observation IDs. Its
definition, condition-dependent basal diffraction/hydration, occurrences and
qualified geological interpretations are retained. No COD assignment is
required for these additions. Source access, original page/section locators,
specimen/occurrence scope and remaining gaps remain explicit.

Within the 1,993 records without attached COD relationships, 519 now have
publication observations and 1,474 have none. The new broader first-pass
checklist has one reviewed record and 1,992 pending; earlier partial research
is preserved. The resumed session totals nine records, 201 evidence rows and
179 source observations, with the same twelve COD models. Public approval
and exhaustive profile completion are separate from this research progress.

The [resumed report](../data/pilots/cod-crystallography-v1/parallel-review-v1/2026-10-06-12h/resumed-2026-10-08/report.md)
records exact sources, saved IDs, backup, receipt and next queue. Preservation,
SQLite integrity and foreign-key checks passed; zero conflicting primary
targets and the public catalogue unchanged. Continue broader reading with
Allophane, then Aluminocerite-(CeCa) and Aluminocopiapite, excluding saved
records. The earlier COD-only queue for Adelite, Allactite and Augelite remains
parked in progress.json. No new automation was requested.

## Parallel direct review — 6–7 October 2026

The user authorized twelve hours of direct reading and record enrichment,
ending **7 October 2026 at 08:41:32 Budapest**. Three reviewers own disjoint
packets of 46 formula cases each (138 minerals / 3,009 candidate comparisons).
The coordinator checks conclusions and is the sole database writer. A
same-chat scheduled continuation resumed this effort every 30 minutes, with
a final reporting run after the review deadline; it is now paused. Local execution requires
the computer, Codex app and existing managed containers to remain available.

Reviewers read original descriptions and compare composition, hydration,
crystal form, specimen origin and measurements. Small commands retrieve,
format and validate evidence; no scraper, embeddings or new matching program
has been added. Useful laboratory and related structures retain their source
conditions. Original-publication details can enrich a record without a COD
assignment. The three disjoint main publication queues contain 553 completed
reviews. Focused and coordinator reviews bring publication coverage to 561
distinct minerals. All completed reviews are committed; the continuation is paused.

| Final twelve-hour result through checkpoint 92 | At dispatch | Final |
|---|---:|---:|
| Minerals with useful COD relationships | 4,075 | 4,233 |
| Publication-supported identified minerals | 41 | 59 |
| Primary COD assignments | 44 | 68 |
| Formula cases awaiting first review | 138 | 0 |
| Qualified structure observations | 44 | 2,602 |
| Conflicting primary assignments | 0 | 0 |

The 92 saved checkpoints enriched **685 distinct mineral records / 20,756 evidence entries**:
All 138 formula cases have a saved first review. Further records gained publication details and qualified structure references. Lithiotantite,
Håleniusite-(La), Iseite, Goldschmidtite, Kaliophilite, Hemleyite and Karwowskiite gained natural specimen assignments,
confirmed against original papers and revision-specific COD files.
561 minerals gained 17,399 source-qualified publication observations.
Wadalite, historical Allende Louisfuchsite, Katayamalite and Serpierite also
gained verified natural primary assignments. Historical names, symmetry
corrections, specimen substitutions and model defects remain preserved.
The [final progress report](../data/pilots/cod-crystallography-v1/parallel-review-v1/2026-10-06-12h/final-report.md)
records exact counts and remaining research; 1,993 registry minerals still
lack a useful COD relationship in this saved review.
Original natural Åsgruvanite-(Ce) and Akasakaite-(Ce) CIFs and published
Bergbauerite and Arzamastsevite atomic-coordinate tables were compared and saved.
Piilonenite-(Nd) retains its verified original natural CIF, with model composition,
missing hydrogen coordinates and conflicting reported refinement details preserved.
Maohokite retains both its original natural diffraction interpretation and the
subsequent competing structural proposal. These enrichments do not imply primary
COD identities. Lechatelierite retains explicitly qualified silica comparison
models; periodic proxy cells are not asserted as a universal glass lattice.
Hokkaidoite has two actual
benzo[ghi]perylene crystal counterparts and 80 preserved rejected hints.
Reference-cell measurements for
laboratory counterparts remain distinct from the target mineral's geometry.

Backups and atomic preservation checks passed. The public catalog and prior
evidence remain intact. All completed agent findings are saved; broader
research remains queued. The
[progress report](../data/pilots/cod-crystallography-v1/parallel-review-v1/2026-10-06-12h/report.md),
protocol, dispatch manifest, coordinator status, reviewer notes and receipts
are under `data/pilots/cod-crystallography-v1/parallel-review-v1/2026-10-06-12h/`.

## Purpose and decision boundary

This pilot tests one narrow, reproducible adapter from the Crystallography Open
Database (COD) to three already-defined questionnaire questions:

- `crystallography.space_group`;
- `crystallography.unit_cell`; and
- `crystallography.structure_references`.

It tests whether revision-pinned COD observations can be crosswalked to
existing Waajacu minerals, normalized without losing scientific context, and
reviewed at an acceptable cost. It does not assert that the catalog is already
enriched, that every mineral has a COD determination, or that one observed
structure is a timeless species-wide constant.

The adapter is mechanical, not epistemic. It may discover records, parse CIF,
preserve raw values, normalize approved forms, validate schemas, calculate
hashes, and produce review queues. Model memory, an inferred mineral match, or
a locally calculated value is not scientific evidence.

This is a private technical pilot. COD remains `pilot_only`, the broader
crystallography module remains `not_launched`, and none of the pilot's values,
coverage states, preferences, or citations may enter a public projection. A
later admission and publication decision is separate from successfully running
this adapter.

## Official semantic baseline

The adapter is governed by primary COD and IUCr documentation rather than by
examples remembered by an implementer:

- COD's [REST API](https://wiki.crystallography.net/RESTful_API/) defines
  search formats, search flags, and revision-addressable entry access.
- The [COD MySQL schema](https://wiki.crystallography.net/cod_mysql_schema/)
  documents indexed cell, symmetry, status, duplicate, optimality, date, and
  repository-revision fields.
- The IUCr publishes the current
  [CIF core dictionary releases](https://www.iucr.org/resources/cif/dictionaries/cif_core)
  and the normative [cell](https://www.iucr.org/__data/iucr/cifdic_html/1/cif_core.dic/Ccell.html)
  and [space-group](https://www.iucr.org/__data/iucr/cifdic_html/1/cif_core.dic/Cspace_group.html)
  definitions.
- IUCr's [CIF placeholder specification](https://www.iucr.org/resources/cif/spec/ancillary/placeholders)
  defines the distinct unquoted `?` and `.` tokens.

The exact dictionary tag used by a source value is part of provenance. Modern,
legacy, and COD-curated aliases are not silently collapsed at extraction time.

## Immutable evidence identity

One accepted observation must point to immutable source bytes, not merely to a
live COD page. Every pilot item therefore carries a shared evidence envelope
containing at least:

- `cod_id`, preserved as the exact seven-character text identifier, including
  any leading zero;
- `cod_svn_revision`, a positive integer repository revision. It is not treated
  as a per-entry sequence number;
- the exact revision-pinned URL
  `https://www.crystallography.net/cod/{cod_id}.cif@{cod_svn_revision}`;
- SHA-256 of the retrieved CIF bytes;
- the exact CIF data-block code;
- retrieval time in RFC 3339 form;
- COD status and determination method as reported or explicitly `unknown` in
  private operational metadata;
- duplicate, optimal/suboptimal, and same-structure relations where COD
  supplies them; and
- source status, determination method, material origin, and duplicate,
  optimality, or same-structure relations needed for review.

The completed run manifest separately binds the frozen query, parser, schema,
vocabulary, and adapter-configuration versions used to produce the candidate
claim.

An unqualified `.../{cod_id}.cif` URL points to the newest live revision and is
not sufficient evidence for a frozen claim. A retrieval run stores the raw
bytes before parsing, then records their hash. Identical raw bytes and frozen
configuration must reproduce byte-identical canonical scientific payloads.

The evidence envelope belongs to the COD determination and carries its exact
data block. It is shared by the three question answers derived from that
determination, while each extracted scalar retains its exact source tag and raw
lexeme.

## Reproducible 96-mineral sample

The pilot follows the questionnaire's mixed sample so ordinary records and
difficult cases are both represented. The sample consists of 60 deterministic
baseline minerals and 36 predeclared challenge minerals, split into 72
development and 24 holdout records.

### Baseline selection

For every existing mineral `public_id`, hash the UTF-8 bytes of the ASCII salt
`questionnaire-v1-baseline`, one literal zero byte (`0x00`), and the exact
`public_id`. Rank by the 32 SHA-256 digest bytes ascending, then by UTF-8
`public_id` bytes as the tie-breaker. The first 45 IDs form the development
baseline and the next 15 form the holdout baseline.

### Challenge selection

The remaining 36 records come from a versioned eligibility manifest:

- 12 nomenclature/status cases;
- 12 value-shape/applicability cases; and
- 12 provenance/risk cases.

Candidate discovery may be used to identify records that genuinely qualify for
these strata. Discovery cannot inspect or use normalized adapter output. Each
eligibility entry records the existing `public_id`, group and stratum,
qualifying raw evidence, rule version, and curator.

The complete eligibility manifest is frozen and hashed **after candidate
discovery but before any normalized adapter output is inspected**. Eligible IDs
are then ranked with the salts and quotas defined by the questionnaire. Nine
records from each 12-record challenge group enter development and three enter
holdout. Together with the baseline, the final split is therefore:

| Set | Baseline | Challenge | Total |
|---|---:|---:|---:|
| Development | 45 | 27 | 72 |
| Holdout | 15 | 9 | 24 |
| Total | 60 | 36 | 96 |

The adapter mapping and configuration are frozen after development and before
holdout processing. If holdout inspection requires a systemic mapping change,
the questionnaire's replacement-holdout protocol applies; the inspected
holdout is not quietly reused to tune the adapter.

The 96 records are mineral profiles sampled from the fixed population. COD
useful-yield calculations use the predeclared eligible denominator: sampled
minerals having at least one manually reviewed, accepted COD-to-mineral
crosswalk. A mineral with no eligible COD determination is not grounds to
invent a value, relax identity rules, or add another mineral.

## Species crosswalk contract

Crosswalking is a reviewed scientific decision, not a join on a normalized
name. Every COD determination receives an explicit decision of `accepted`,
`needs_attention`, or `rejected` against the existing mineral registry.

An accepted decision must satisfy all of these rules:

1. It targets exactly one existing mineral `public_id` from the frozen
   population. The adapter cannot create a mineral or change identity data.
2. It retains the exact COD ID, revision, data block, candidate signals,
   decision basis, reviewer, review time, and any ambiguity flags.
3. At least one explicit mineral-identification signal is present, such as a
   CIF mineral-name field, an admitted authority alias, or identification in
   the primary structure publication.
4. A person reviews the pinned CIF and supporting identity evidence and records
   the acceptance. **Every accepted crosswalk is manually reviewed.**

Fuzzy matching is forbidden for pilot crosswalk candidates. Machine candidate
generation uses only an exact
authority-accepted name or exact admitted authority alias. Formula is retained
as a diagnostic and cannot create or accept a candidate; an explicit mineral
identification in the primary publication may be added only through review.
The offline research report may separately preserve case, diacritic,
punctuation, and spelling hints. These are unreviewed leads, not pilot
candidates or accepted crosswalks, and do not enter the frozen sample or
useful-yield count merely because their names resemble a mineral.
A missing identifier, polymorph/polytype ambiguity,
group/end-member ambiguity, synthetic analogue, renamed species, conflicting
publication identity, or more than one plausible `public_id` routes the item to
`needs_attention` or `rejected`.

The crosswalk is directional: it permits a COD observation to support a claim
about an existing Waajacu subject. It never authorizes COD to overwrite the
IMA-owned canonical name, formula, nomenclature status, aliases, or authority
identifiers.

## CIF value rules shared by all three questions

### Placeholders and missingness

In CIF, an unquoted `?` means that the value is unknown and an unquoted `.`
means that the item is inapplicable. They are different tokens under the
[IUCr placeholder rules](https://www.iucr.org/resources/cif/spec/ancillary/placeholders).
Neither is a numeric zero, empty string, JSON `null`, or literal scientific
answer. The adapter preserves the raw token as a private extraction outcome;
it does not create a questionnaire coverage row while the module remains
`not_launched`. A quoted question mark or full stop is ordinary text, not a
placeholder.

Absence of a tag is also not proof of inapplicability. It means only that the
particular reviewed CIF did not report that item unless another admitted source
and rule establish more.

### Numeric values and uncertainty

Each numeric object preserves:

- normalized numeric `value`;
- canonical `unit`;
- optional standard uncertainty;
- the exact raw CIF lexeme, such as `5.431(2)`;
- the exact source tag; and
- linkage to the containing data block through the shared evidence envelope.

Parenthesized CIF uncertainty is parsed according to its decimal place, not as
an independent absolute number. Separate uncertainty fields are retained when
present. If appended and separate uncertainties disagree, the adapter does not
choose one silently: the observation becomes a blocking review item.

Normalization never deletes the raw lexeme. Unit conversions, if later
approved, record the raw value/unit and transformation version. No calculated
value is presented as though COD reported it.

## `crystallography.space_group`

A space group is a repeatable observation attached to one pinned COD
determination and its conditions. It is not a single unqualified species-wide
field.

For ordinary three-dimensional crystallography, an observation may retain:

- Hermann-Mauguin symbol, including raw, normalized, exact tag, and whether
  the source used an alternate, full, or legacy form;
- Hall symbol with raw and normalized values and exact tag;
- International Tables number from 1 through 230, with raw value and exact
  tag;
- International Tables coordinate-system code when reported;
- provenance role distinguishing COD-curated values from author-reported
  original values; and
- consistency state: `consistent`, `unverified`, or `conflicting`.

The [IUCr space-group category](https://www.iucr.org/__data/iucr/cifdic_html/1/cif_core.dic/Cspace_group.html)
states that the International Tables number identifies a space-group type, not
its coordinate-system setting. A Hall symbol or the actual symmetry operations
are stronger setting identifiers. The adapter therefore does not manufacture
a Hall symbol, origin choice, basis, or setting from an H-M symbol or number.
The IUCr definition of
[`_space_group_name_H-M_alt`](https://www.iucr.org/__data/iucr/cifdic_html/1/cif_core.dic/Ispace_group_name_H-M_alt.html)
also warns that alternate H-M forms are not generally computer-interpretable.

Modern and legacy aliases may be read, including `_space_group.name_H-M_alt`,
`_space_group_name_H-M_alt`, `_space_group.name_H-M_full`,
`_symmetry_space_group_name_H-M`, `_space_group.name_Hall`,
`_space_group_name_Hall`, `_symmetry_space_group_name_Hall`,
`_space_group.IT_number`, `_space_group_IT_number`, and
`_symmetry_Int_Tables_number`. The exact tag always survives. COD-curated
regular tags and `_cod_original_*` author values remain distinct provenance
layers rather than competing strings flattened into one field.

Conflicting symbols/numbers remain visible and enter review. Superspace and
magnetic groups are outside this first adapter schema; encountering one creates
a review item rather than forcing it into the ordinary 1–230 range.

## `crystallography.unit_cell`

A unit-cell answer is a repeatable, determination-scoped observation. A valid
first-pilot cell contains all six independent reported parameters:

- `a`, `b`, and `c`, each greater than zero and expressed canonically in
  angstrom;
- `alpha`, `beta`, and `gamma`, each strictly between 0 and 180 degrees; and
- their raw lexemes, exact tags, and standard uncertainties where reported.

Partial cells route to review. The adapter does not fill missing axes or angles
from assumed symmetry. A reported cell volume may be retained in cubic
angstrom with its raw lexeme, tag, uncertainty, and `reported_in_cif` or
`cod_curated` origin. The sourced claim does not add a locally recomputed
volume. The generic value schema calls these origins `reported_in_source` and
`source_curated`; the COD adapter maps reported CIF and COD-curated values to
those terms respectively.

`Z` is optional and must be a positive integer when reported. Because CIF
defines it relative to a structural, moiety, or sum formula, the adapter also
retains the formula tag and raw formula text used by that determination. It
does not substitute the registry's protected formula.

Conditions travel with the cell. Cell-measurement temperature/pressure and
diffraction-ambient temperature/pressure remain separate condition kinds even
when their numbers match. Values use kelvin and kilopascal canonically, retain
raw lexemes/tags/uncertainties, and preserve any reported thermal or pressure
history. The IUCr dictionary's current
[`_cell_measurement_temperature`](https://www.iucr.org/__data/iucr/cifdic_html/1/cif_core.dic/Icell_measurement_temperature.html)
and [`_diffrn_ambient_temperature`](https://www.iucr.org/__data/iucr/cifdic_html/1/cif_core.dic/Idiffrn_ambient_temperature.html)
definitions are distinct; deprecation of a legacy tag does not authorize
discarding its provenance. Method and radiation context are retained when
reported.

Different reported cells are never averaged. Temperature, pressure, specimen,
method, revision, and refinement differences remain attached to their own
observations.

## `crystallography.structure_references`

Structure references are repeatable relations to publications or other
reported determination documentation. One normalized item represents one
reference supporting the pinned structure determination and may contain
ordered authors, title, journal, year, volume, issue, page/article details,
publication status, and DOI in raw and normalized forms.

The DOI tag is interpreted under the IUCr
[`_citation_doi` definition](https://www.iucr.org/__data/iucr/cifdic_html/1/cif_core.dic/Icitation_doi.html).
General `_citation_*` loops are bibliographies, not automatically lists of
independent structure determinations. Even `_citation_id` equal to `primary`
means the most pertinent citation in that CIF context; it does not prove that
every row is a structure-determination source. The adapter prioritizes the
record's publication fields, including `_publ_author_name`,
`_publ_section_title`, `_journal_*`, and `_journal_paper_DOI`, then presents
ambiguous citation rows for review.

A personal communication or unpublished deposition may lack a complete paper
citation. The immutable pinned COD record can still support the observation,
but the reference must say that its publication status is nonpublished or
unknown rather than fabricating bibliographic details. Software, method, and
background citations are excluded unless review establishes that they support
the specific structural determination.

## Candidate exclusions and review routing

Ordinary public-preference candidates exclude records that COD marks as
duplicates, suboptimal, error-bearing, retracted, or theoretical. Candidate
discovery records these flags instead of discarding their existence. The
[COD REST documentation](https://wiki.crystallography.net/RESTful_API/)
documents explicit inclusion controls for duplicates, errors, and theoretical
records; the pilot query freezes every such option.

These records may still be retained in the private audit set when they explain
a relation or historical change, but they do not count as independent useful
yield and cannot become an ordinary preferred observation. Warnings, partial
cells, contradictory symmetry, unsupported symmetry kind, missing evidence
bytes, hash mismatch, identity ambiguity, and uncertainty disagreement route
to manual review.

The pilot never treats duplicate COD depositions as independent corroborating
sources. An `optimal` relation is selection metadata, not proof that an
observation is a universal species value.

## Revision, correction, and withdrawal behavior

A newer COD revision creates new evidence. It never mutates or silently
replaces the revision pinned by an earlier run.

For every newly observed revision, the adapter freezes the bytes, computes a
new hash, produces a semantic diff, and requires review before any resolution
could change. A warning triggers review. An error or retraction withdraws the
affected observation from any future public resolution while preserving the
old evidence, decision, and event history. Duplicate or suboptimal status
changes likewise change eligibility without erasing prior audit records.

Failure to find a formerly retrieved COD ID in a later search is not proof of
deletion or retraction. It creates an investigation item. Only an explicit,
reviewed source status supports withdrawal behavior.

## No public projection in this phase

Pilot artifacts live in private staging/quarantine and review reports. The run
must not:

- write directly to the public catalog database;
- activate the crystallography module;
- resolve a preferred species-wide crystallography value;
- emit public search/filter/index fields;
- create mineral-level public missing-state rows; or
- expose raw CIF or adapter-review material through public endpoints.

Even a technically perfect run leaves COD `pilot_only`. Publication requires a
separate source-admission revision, module lifecycle decision, approved public
resolution policy, and successful public-export checks.

## Current ingestion blocker

The current ingestion implementation cannot safely accept this pilot's
payload. Its granular evidence scopes are limited to `identity.*`,
`identifiers.*`, `properties.*`, and `safety.*`; `crystallography.*` is
rejected. Its bulk policies, `create_only_v1` and `ima_identity_v1`, are built
for record creation or IMA-owned identity ingestion rather than for attaching
repeatable scientific claims to fixed existing minerals.

Consequently, this pilot must not be forced through `properties.*`, imported as
identity data, or written directly into database tables. Before staging
normalized COD claims, the application needs a dedicated
`cod_crystallography_v1` **claim-only** ingestion path that:

- resolves only existing `public_id` values and refuses record creation;
- refuses changes to names, formulas, aliases, status, authority identifiers,
  or any other protected identity field;
- admits exactly the three question keys in this pilot;
- validates repeatable values against their versioned JSON schemas;
- requires the immutable COD evidence envelope, raw value/tag provenance,
  source rights, crosswalk decision, and manual-review metadata;
- preserves multiple qualified observations and conflicts without overwrite or
  averaging;
- stages every item in private quarantine with deterministic batch hashes,
  diff, idempotency, and reject/attention states; and
- has no public projection path while the source decision is `pilot_only` or
  the module is not active.

Implementing that path is application work, not permission to retrieve or
publish content.

## Standard discovery workflow

Candidate discovery is a versioned Rust workflow, not a collection of
per-mineral Python scripts. Its offline preparation step validates the bound
pilot and every schema hash, validates the committed 6,226-mineral public
catalog, opens only its content-addressed SQLite file in read-only/query-only
mode, and produces four canonical artifacts:

- `population-snapshot.json`, containing the exact existing public IDs and
  canonical-name snapshots;
- `baseline-selection.json`, containing the deterministic 45-development and
  15-holdout baseline;
- `cod-query-plan.json`, containing the frozen COD metadata requests; and
- `preparation-manifest.json`, binding the other three artifacts and their
  inputs by exact SHA-256 and byte count.

Prepare and independently reproduce those files from the repository root:

```bash
mkdir -p data/pilots/cod-crystallography-v1
cargo run --locked -p minerals-cod-pilot --bin cod-pilot -- prepare \
  --repo-root . \
  --output data/pilots/cod-crystallography-v1/preparation-v1

cargo run --locked -p minerals-cod-pilot --bin cod-pilot -- verify \
  --repo-root . \
  --input data/pilots/cod-crystallography-v1/preparation-v1
```

The output directory is private, mutable pilot state and is ignored by Git.
`prepare` and `verify` perform no network requests, never open
`data/minerals.db`, and never write a database.

Retrieval is a separate, explicit and resumable command. A bounded first shard
is useful as a transport/wire-format check; omit `--max-new-requests` only when
continuing the complete discovery run:

```bash
cargo run --locked -p minerals-cod-pilot --bin cod-pilot -- fetch \
  --repo-root . \
  --prepared data/pilots/cod-crystallography-v1/preparation-v1 \
  --pilot-root data/pilots/cod-crystallography-v1 \
  --max-new-requests 1

cargo run --locked -p minerals-cod-pilot --bin cod-pilot -- verify-execution \
  --repo-root . \
  --prepared data/pilots/cod-crystallography-v1/preparation-v1 \
  --pilot-root data/pilots/cod-crystallography-v1
```

Every invocation first regenerates and verifies the preparation. Existing
receipts and source objects are rehashed before resume; offline verification
never refetches missing or damaged bytes. The frozen 12-second request-start
interval means that a clean 1,000-shard run has a minimum request cadence of
about 3 hours 20 minutes, before response time or retries. This deliberately
favors the source service over a fast bulk scrape.

The metadata plan partitions the complete seven-digit COD namespace into the
1,000 explicit, non-overlapping prefixes `000%` through `999%`. Each request
includes duplicates, error-marked records, and theoretical records so those
facts remain available for review; there is no undocumented
`include_suboptimal` parameter. The requests are sequential, redirect-free,
rate-limited, bounded, resumable, and stored as exact content-addressed bytes.
Text/name searches are not used: COD's `text` search is a keyword search, so
identity candidates are computed offline against the frozen population after
the complete raw metadata snapshot exists.

Neither successful preparation nor successful retrieval is a crosswalk,
scientific answer, ingestion batch, or publication decision. Challenge
eligibility is separately reviewed and frozen before normalized adapter output
may be inspected.

### Offline mineral matching and coverage

Run matching inside the existing managed admin container:

```powershell
docker compose exec -T --user 0:0 admin bash tools/container-task.sh pilot-test
docker compose exec -T --user 0:0 admin bash tools/container-task.sh pilot-match
docker compose exec -T --user 0:0 admin bash tools/container-task.sh pilot-verify-matching
```

`pilot-match` verifies the saved preparation, execution index, recovery history,
and raw response hashes, then processes every metadata row offline. Literal
case-sensitive `mineral` labels are compared with frozen canonical names. The
current frozen population has no reviewed authority-alias artifact; this
limitation is reported rather than filled with inferred aliases.

Separate exploratory leads use case/diacritic/punctuation folding and at most
one insertion, deletion, or substitution for folded names 5–96 characters long.
The metadata `mineral`, `commonname`, and `chemname` fields can supply these
leads. They never establish mineral identity or become accepted crosswalks.
Formula, source status, duplicate/optimal relations, synthetic/theoretical
labels, and publication metadata remain raw diagnostic context.

Reports live in ignored private state under
`data/pilots/cod-crystallography-v1/candidate-matching-v1/<input-fingerprint>/`:

- `source-inventory.jsonl`: every original metadata row, its COD ID/revision,
  hashed source-body locator and JSON row pointer, raw diagnostic fields,
  proposed links, and review flags; unmatched and repeated rows are retained;
- `review-queue.jsonl`: rows with exact candidates or exploratory leads, all
  explicitly unreviewed;
- `mineral-coverage.jsonl`: all 6,226 minerals, candidate/lead counts, and
  further-research states; these are discovery metrics, not questionnaire
  coverage or assertions of absence;
- `report.md`: readable counts, limitations, and the next review task; and
- `matching-manifest.json`: frozen input, matcher-code, policy, and output
  hashes and byte counts.

Identical inputs reproduce the same artifact bytes. A repeat `pilot-match`
verifies and reuses the existing report; `pilot-verify-matching` reproduces its
hashes without writing a new report. Changed inputs or matcher code produce a
new report directory. Failed writes retain their staging directory. Original
responses and earlier reports are never overwritten or removed.

Matching retrieves no CIFs, normalizes no crystallographic values, changes no
mineral identities, and writes no database. Review the proposed links and their
source publications before preparing challenge eligibility and freezing the
96-record pilot selection.

### Reviewed connection recovery (execution index v2)

The original query plan and preparation remain frozen, including their
four-attempt limit. Exhausting that limit still stops `fetch`; there is no
automatic reset or indefinite retry. After diagnosing connectivity, an operator
can record an explicit exception with the offline `recover-transport` command.
It accepts only four completed connection/timeout failures with no HTTP
response, after at least five minutes of cooldown. HTTP failures, invalid
responses, body-read failures, interrupted reservations, damaged evidence, and
validation failures cannot use this recovery path.

Recovery verifies existing evidence, retains the exact failed execution index
as immutable content-addressed bytes, and records a reviewer, reason,
timestamp, and the failed attempts. Every successful receipt and attempt count
is preserved. Each shard may receive only one additional four-attempt cycle;
at most eight shards may receive reviewed recovery in one run.

Recovered indices use the separate
[`execution index v2 schema`](../schemas/pilots/cod-metadata-discovery-execution-index-v2.schema.json).
They set the assertion that the original retry policy was enforced to `false`
and assert enforcement of the reviewed recovery policy instead. The completed
v2 snapshot hash also binds the recovery history. The original v1 schema,
scientific contract, query hashes, inclusion flags, response limits, 12-second
cadence, and population remain unchanged.

```bash
docker compose exec -T --user 0:0 admin bash tools/container-task.sh \
  pilot-recover-transport --reviewer 'operator-id' \
  --reason 'Connection diagnosis supporting another bounded cycle'
docker compose exec -T --user 0:0 admin bash tools/container-task.sh pilot-fetch
```

Recovery performs no network requests; the separate `pilot-fetch` command
resumes retrieval. Both commands share the same exclusive fetch lock. Transport
logs include the shard, attempt number, and underlying error chain.

Execution index v3 additionally permits a generic `request` error only when the
review supplies the private `runs/<run>/run.log` and its exact shard, attempt,
URL, `SendRequest` cause, and TLS `close_notify` failure establish that the
connection closed before response headers. The complete log is stored in CAS
and bound into the review and snapshot identity. Incorrect, missing, duplicate,
or unrelated log evidence is rejected. The v1 and v2 schemas remain unchanged;
the same cooldown and recovery limits apply.

For this case, append `--failure-log data/pilots/cod-crystallography-v1/runs/<run>/run.log`
to the recovery command. The client retains no idle connections between its
deliberately spaced requests, to avoid reusing connections that COD may have
closed. This mitigates one observed failure mode; it cannot guarantee source
availability. Certificate verification remains enabled. Body errors now retain
their `body_` phase in the error kind and are ineligible for reviewed recovery.

## Staged next sequence

The pilot proceeds in this order:

1. Validate the now-approved three value schemas, COD evidence envelope,
   crosswalk-decision schema, selection and run manifests, units, source tags,
   and content hashes as one bound contract.
2. Implement a versioned Rust private-pilot runner for candidate discovery,
   content-addressed raw artifacts, deterministic selection, parsing, and
   review output. It writes no database records and has no approval authority.
3. Run metadata-only candidate discovery with a frozen COD query and inclusion
   flags. Do not normalize CIF values yet.
4. Produce and manually review crosswalk decisions against existing
   `public_id` values. Freeze the discovery result and crosswalk artifact.
5. Build, review, freeze, and hash the challenge eligibility manifest before
   inspecting any normalized output; then compute the complete 96-record split.
6. Retrieve only revision-pinned CIFs selected for the sample, preserve exact
   bytes, and record SHA-256, data blocks, retrieval time, status, and relations.
7. Parse and review the 72-record development set. Resolve blocking mapping
   decisions and freeze adapter configuration.
8. Process the 24-record holdout without tuning. Compare 100% of values and
   missing/applicability decisions to their pinned raw locators.
9. Produce the private pilot report with useful yield, extraction outcomes,
   error classes,
   review burden, reproducibility, idempotency, unresolved limitations, and a
   pass/revise/reject recommendation.
10. Only after a passing report, implement and test the
    `cod_crystallography_v1` claim-only ingestion path. Confirm that record
    creation, identity mutation, unrecognized question keys, missing hashes,
    and unreviewed accepted crosswalks fail closed.
11. Make separate decisions about COD admission, crystallography-module
    activation, resolution policy, and public release.

This sequence deliberately separates discovery, identity judgment,
normalization, ingestion, and publication. The adapter can remove repetitive
retrieval work without being allowed to make the scientific decisions that
require evidence and review.
