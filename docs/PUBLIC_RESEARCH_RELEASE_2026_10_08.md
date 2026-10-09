# Reviewed public mineral research release — 8 October 2026

The public atlas now presents selected research beside the existing mineral
identity: source observations, qualified COD relationships and reported
crystal models. The layout, colors and artwork are retained. Each observation
shows its source, locator, units and scientific qualifications. Large
collections remain accessible through explicit expansion controls.

This is a bounded, manually reviewed publication, authorized by the user.
It admits the selected source observations in this snapshot. The general COD
pilot, questionnaire modules, safety triggers and occurrence map adapters
remain under their existing admission contracts. No inference from a name or
formula lead becomes an accepted mineral identity.

## Publication controls

The private database retains all original evidence and useful candidates.
Its `properties_json.public_research` contains a separate release projection,
with explicit `publication_status: published` and `review_status: reviewed`.
Only this approved projection supplies public research sections or an
editorial description. Private drafts, review histories, source archives,
transport metadata and complete atomic tables remain private.

Selected evidence retains its original claim and historical source snapshot.
An explicit publication marker and a curated `public_claim` provide the public
payload and reviewed license. This allows a newly checked license to be
recorded without rewriting the original provenance. Private/draft/rejected
flags take precedence over a publication marker. The public search index is
rebuilt from the exported fields rather than private working notes; public
evidence counts reflect the exported claims.

The curated public copy also carries complete source credits: saved authors
when available, otherwise the named source provider, the work title and URL,
license URL, change notice, nonendorsement notice and output license. These
credits are validated before export. Older partial source snapshots remain
unchanged; they cannot bypass the normal complete-attribution requirement.

The release selects compatible CC-BY, CC0 and public-domain factual
observations. Findings with unknown, noncommercial or no-derivatives reuse
terms remain available privately for subsequent rights review. Source links
point to the work; this snapshot does not reproduce article files or figures.
The Aliettite introduction is an individually reviewed original editorial
synthesis with three primary references, separately licensed CC-BY 4.0.
Its private draft and supporting observations remain preserved.

COD describes its database and data as CC0. Original structure authors,
publication titles and available bibliographic details are acknowledged in
the public projection. [COD source policy](https://www.crystallography.net/cod/)

Three EJM newsletters with stale unknown-license metadata were independently
rechecked against their explicit CC-BY 4.0 notices: [Newsletter 52](https://ejm.copernicus.org/articles/32/1/2020/),
[Newsletter 53](https://ejm.copernicus.org/articles/32/209/2020/), and
[Newsletter 54](https://ejm.copernicus.org/articles/32/275/2020/).
Their linked corrections do not concern the selected mineral observations.
Older source snapshots are kept unchanged.

## Scientific scope

COD references distinguish name leads, candidates, assessed related models,
counterparts, unresolved relationships and identified phases. A primary
association identifies the one mineral target for a COD entry; it does not
turn specimen measurements into universal species constants.

Models retain source formula, cell, symmetry, origin, experimental conditions,
uncertainties and validation state. Most were compared through saved COD
metadata and still require inspection of their revision-pinned CIF. The page
shows this limitation. Missing temperature or pressure remains unreported;
pressure values whose units need a CIF check retain that warning. Synthetic
and related structures retain their relationship labels before measurements.

Protected IMA names, formulas, identifiers, nomenclature and references remain
unchanged. A formula reported in a study is displayed as a source observation,
not a replacement for the official identity.

## Release verification

The coordinator prepares and applies the selection in a backed-up atomic
transaction. Full earlier records and unrelated tables are checked for
preservation, alongside integrity, foreign keys and zero conflicting primary
COD targets. Exact selected IDs, original row digests, source decisions,
backup and commit receipts remain in the ignored private review folder.

The public snapshot is rebuilt from the private database in the existing
managed admin container, validated and inspected. Its manifest and exact
Brotli/gzip representations form the checked-in `public-catalog` source
package. GitHub Pages reconstructs the complete SQLite database, verifies its
hash, size and schema, and requires both compressed files to contain those same
bytes. The full raw export is also preserved privately. GitHub's single-file
limit therefore does not remove any selected scientific data. The public
website receives no private database, reviews or source archives.

The validated snapshot contains:

| Public content | Entries | Minerals covered |
| --- | ---: | ---: |
| Mineral identities | 6,226 | 6,226 |
| Approved research profiles | 4,680 | 4,680 |
| Source observations | 7,250 | 480 |
| Qualified COD references | 16,131 | 4,233 |
| Reported crystal models | 2,614 | 213 |
| Original editorial introductions | 1 | 1 |
| Evidence associations, including IMA identity | 31,383 | 6,226 |

The 76 primary COD entries belong to 65 mineral targets, with zero conflicting
assignments. Publication does not upgrade candidate or metadata-only findings
to scientific verification. All 17,578 private publication observations remain
preserved, including material withheld from this public snapshot. The final
source comparison removed two incorrectly attributed public observations and
four raw-only model payloads from the compact public view. Their complete
original private evidence remains available; supported natural measurements
and compact model facts retain their accurate sources.

Manifest generation: `2026-10-09T05:39:19.697Z`.
Release identity: `sha256:386bd57064778108c8587ca74074c8d2ff39af92274fe30ca6e64c341554e66e`.
Raw database: `217,604,096` bytes,
`sha256:51bb6d9848440a427f96a1000f976efcf4d3384763ac17114a19bcbfb521374b`.
The byte-equivalent Brotli and gzip source files are respectively `12,713,937`
and `18,794,700` bytes. Independent reconstruction and public SQLite content
scanning found no private-data or credential violations.
