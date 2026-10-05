# COD crystallography adapter pilot v1

Status: **private technical contract; retrieval not started**

Source decision: **`pilot_only`**

Population: the existing 6,226 valid mineral species in the questionnaire
baseline. This pilot must not add, merge, rename, withdraw, or otherwise change
that population.

Questionnaire:
[`docs/MINERAL_RECORD_QUESTIONNAIRE_V1.md`](MINERAL_RECORD_QUESTIONNAIRE_V1.md)

Source admission decision:
[`docs/MINERAL_SOURCE_ADMISSION_V1.md`](MINERAL_SOURCE_ADMISSION_V1.md)

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

Fuzzy matching is forbidden. Machine candidate generation uses only an exact
authority-accepted name or exact admitted authority alias. Formula is retained
as a diagnostic and cannot create or accept a candidate; an explicit mineral
identification in the primary publication may be added only through review.
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
