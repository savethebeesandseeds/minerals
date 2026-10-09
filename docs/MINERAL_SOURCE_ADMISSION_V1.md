# Mineral source admission decisions v1

Status: **draft decisions for pilot validation**

Reviewed: **2026-08-28**

An individually reviewed public research snapshot was authorized on 8 October
2026. Its bounded source admission and retained qualifications are recorded in
[the public research release note](PUBLIC_RESEARCH_RELEASE_2026_10_08.md).
That release selects specific licensed observations and COD references; it
does not promote the general pilot adapters or change the fixed IMA identity
population. The pilot and module decisions below continue to govern automatic
ingestion.

Questionnaire: [mineral record questionnaire v1](MINERAL_RECORD_QUESTIONNAIRE_V1.md)

Normative matrix:
[`schemas/mineral-source-admission-matrix-v1.json`](../schemas/mineral-source-admission-matrix-v1.json)

## Outcome

The first rights-and-fit review does **not** support a single bulk source for
every missing catalog field.

- The existing IMA-CNMNC release remains the admitted owner of protected
  identity and authority context.
- Four sources are bounded private-pilot candidates: COD may proceed once its
  pilot plan is approved; Smithsonian NMNH, USGS MRDS, and the NIOSH Pocket
  Guide must first pass their remaining pilot-adapter prerequisite gates.
- The Handbook of Mineralogy is private-reference-only under current
  reproduction terms.
- RRUFF is deferred until an express redistribution license or written
  permission is recorded.
- Mindat is rejected for this workflow under its current terms.

`pilot_only` is not permission to publish. A bounded private adapter may begin
only after the publisher/work/release, rights, stable-identity, and
field-ownership gates pass. The pilot then freezes the exact raw bytes and
configuration, records SHA-256 hashes, validates locators and semantics, and
measures review burden. Every remaining gate must pass before promotion to
`admitted`; only an `admitted` source may supply a public claim.

## Decision summary

| Source | Decision | Narrow allowed role | Main remaining limit |
|---|---|---|---|
| [IMA-CNMNC Master List](https://cnmnc.units.it/files/editor/IMA_Master_List_%282026-07%29.pdf) | admitted | Protected name, formula, status, IMA numbers, discovery-country context, and authority references | It does not own official mineral symbols, general classification, locality, properties, or narrative. |
| [Crystallography Open Database](https://wiki.crystallography.net/RESTful_API/) | pilot-only | Revision-pinned structure citations, space-group observations, and unit-cell measurements | Species crosswalks and condition-preserving mappings need full pilot review. |
| [Smithsonian NMNH Mineral Sciences](https://collections.nmnh.si.edu/search/ms/) | pilot-only | Type-material relations and specimen-backed occurrence reports | Reuse is gated per record; collection records are not complete species geography. |
| [USGS MRDS](https://www.usgs.gov/publications/mineral-resources-data-system-mrds) | pilot-only | Deposit or occurrence context where a reviewed record explicitly identifies a mineral species | Commodity codes, legacy coverage, third-party inputs, and coordinate precision require review. |
| [NIOSH Pocket Guide](https://www.cdc.gov/niosh/npg/default.html) | pilot-only | Form- and route-specific occupational hazard claims | CAS/group matches cannot become automatic mineral or hand-specimen danger labels. |
| [Handbook of Mineralogy](https://handbookofmineralogy.org/) | private-only | Curator consultation and discovery of primary citations | Current reproduction terms do not admit extraction and republication of handbook summaries. |
| [RRUFF](https://www.rruff.net/rruff-homepage/) | deferred | Candidate specimen-scoped chemistry and structure observations | No express database redistribution license was found on the official service. |
| [Mindat](https://www.mindat.org/terms.php) | rejected | None under current terms | Current terms prohibit the systematic extraction/database-population and AI/TDM workflow required here. |

## Rights and scientific boundaries

### IMA-CNMNC

The July 2026 master list states CC BY-SA 3.0 terms. Waajacu already retains a
frozen artifact, retrieval time, parser/configuration identity, record hash,
review report, attribution, changes notice, and non-endorsement notice for the
approved release. The source contains 6,227 rows; the fixed public population
continues to expose only the 6,226 valid species.

Official IMA mineral symbols belong to a separate CNMNC work. No symbol source
is admitted by this matrix yet, so `identifiers.ima_symbol` remains in the
questionnaire without being attributed to the July 2026 master list.

Profile enrichment cannot use another source to overwrite these protected
fields. A future IMA release remains a separate nomenclature/identity change,
outside this fixed-population effort.

### Crystallography Open Database

COD describes its data/database as CC0 and provides structured REST and
[bulk access](https://wiki.crystallography.net/howtoobtaincod/), stable COD
identifiers, revisioned records, CIF data, and scholarly citations. Original
structure authors should still be acknowledged.

COD values are observations, not timeless universal species constants. Several
records for one mineral can legitimately differ by specimen, pressure,
temperature, method, or refinement. The pilot therefore keeps those claims
separate and does not average them.

### Smithsonian NMNH

Smithsonian [Open Access guidance](https://www.si.edu/openaccess/faq) documents
structured access and CC0 reuse for designated records, while NMNH notes that
not all collection data is necessarily CC0. The pilot must retain the
accession/catalog number, record-specific rights designation, type status,
specimen basis, locality wording, and any sensitivity/coarsening decision.
`history.type_localities` additionally requires the record or its cited primary
description to designate the type locality explicitly.

A specimen locality supports one specimen-backed occurrence. It does not prove
that the catalog has captured the mineral's complete range.

### USGS MRDS

USGS works are generally public domain, subject to
[third-party exceptions](https://www.usgs.gov/data-management/data-licensing).
MRDS is a legacy deposit database, not a mineral-species authority. A stable
`dep_id` is a useful occurrence locator, but a commodity label cannot be
silently crosswalked to a mineral. The pilot must freeze a named API/DOI
snapshot and retain original references.

### NIOSH Pocket Guide

CDC explains that most agency-created material is public domain while marked
third-party material remains restricted. NIOSH records can support narrow
occupational guidance only when the named substance, polymorph/group coverage,
particle or material form, and exposure route are reviewed explicitly. Missing
guidance is never evidence that a mineral is safe.

### Sources not admitted for public claims

The Handbook can guide a curator toward primary literature, but its summaries
cannot be copied into the public catalog without permission. Public download
availability on RRUFF is not treated as a redistribution grant. Mindat's
current terms make systematic extraction inadmissible, so no scraper, API
adapter, model-assisted extraction, or derived catalog payload may be built
from it.

## What this means for the first content pilot

1. Keep the IMA projection unchanged and use it only as the identity
   crosswalk/authority baseline.
2. Start with COD's three narrow crystallography questions:
   `crystallography.space_group`, `crystallography.unit_cell`, and
   `crystallography.structure_references`.
3. Use the approved executable contracts in the
   [private COD crystallography pilot](COD_CRYSTALLOGRAPHY_PILOT_V1.md); they
   bind the value schemas, condition fields, crosswalk procedure, immutable
   evidence, 96-mineral sample, and useful-yield denominator without launching
   the module or admitting the source.
4. Run frozen metadata candidate discovery, then freeze the complete selection
   manifest before inspecting normalized output. Pin and hash every selected
   CIF revision before parsing.
5. Review every crosswalk and every extraction outcome against its pinned COD
   record/CIF and cited
   publication.
6. Add Smithsonian, MRDS, and NIOSH only as separate pilots; do not combine
   their claims into the COD release.

This first source set still leaves broad physical properties, a universal
chemical/structural classification, narrative history, and comprehensive
occurrences without a clean reusable source. Those modules remain
`not_launched`. They must not be filled from model memory, marked with a literal
`unknown`, or populated from restricted sources. Publicly, the interface should
explain the unavailable module. Per-mineral states may be exercised privately
when a module enters `pilot`, but no module content reaches public surfaces
until the module is `active` and its definitions are approved.

## Decision maintenance

Each matrix row is a source × question-group decision. Change the decision
when—and only when—the evidence changes:

- a new source release or terms revision creates a new reviewed row/revision;
- written permission is stored as rights evidence rather than paraphrased;
- a source may own only the enumerated question keys and occurrence fields;
- every public ingestion requires frozen bytes, retrieval time, SHA-256,
  parser/configuration identity, exact locators, and a reviewable diff; and
- a weighted quality score can never override failed rights, identity,
  locator, semantics, reproducibility, or ownership gates.
