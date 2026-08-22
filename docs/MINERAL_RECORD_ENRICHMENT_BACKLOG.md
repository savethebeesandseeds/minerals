# Mineral record enrichment backlog

> **Call for a later content effort:** enrich the mineral records as one
> coordinated, source-reviewed release instead of adding isolated fields one at
> a time. Occurrence/locality data should be the first map-aware addition.
> Images remain a separate later project with their own provenance and asset
> pipeline.

## Why this is deferred

The current public records are strongest on identity, nomenclature, source
context, evidence, and offers. Reliable localities, physical properties,
crystallography, and media require additional sources, licensing review,
normalization, conflict handling, and human review. Doing those together avoids
repeated schema changes and prevents incomplete fields from looking
authoritative.

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

## Other record content to add in the same enrichment pass

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
- explicit “unknown” versus “not yet researched” states.

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

## One-pass delivery checklist

1. Choose reviewed sources and record their reuse terms.
2. Finalize occurrence and expanded-fact schemas, including uncertainty and
   sensitive-location policy.
3. Build deterministic import adapters that preserve raw values and source
   locators.
4. Add duplicate/conflict checks and a curator review interface.
5. Review a representative pilot across mineral families and countries.
6. Import the full batch and freeze a reproducible source manifest.
7. Extend the sanitized public snapshot and worker contract without exposing
   private review data.
8. Add record maps, geographic filters, and accessible non-map fallbacks.
9. Publish coverage and limitations so missing localities are not interpreted
   as absence.
10. Treat the later image pipeline as its own reviewed release.

## Completion criteria

The enrichment effort is complete only when every published value can answer:
“what does this mean, where did it come from, how precise is it, may we reuse
it, and who reviewed it?” The public map must remain useful without color,
pointer input, or exact coordinates, and mineral records must remain usable when
the map or media package is unavailable.
