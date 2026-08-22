# Map demo data provenance

The checked-in `assets/world_forest_v1.bin` is a fixed 720 x 360 categorical
display asset. SHA-256:
`970e006dac8927e4aa7e659eab20d295f244aab92567f58caf629ce013a7a944`.

Each byte is 255 for water/no estimate, 0 for land where forest is not shown,
or 100 for estimated forest presence. This asset is for a global visual
overview only, not quantitative analysis.

## Global forest cover 2020, version 3

- Producer: European Commission, Joint Research Centre (JRC).
- Citation: Bourgoin, Clement; Verhegghen, Astrid; Ameztoy, Iban; Carboni,
  Silvia; Achard, Frederic; Colditz, Rene (2026), *Global map of forest cover
  2020 - version 3*. European Commission, Joint Research Centre.
  [DOI 10.2905/JRC.354CG88](https://doi.org/10.2905/JRC.354CG88),
  [dataset record](https://data.europa.eu/89h/8c561543-31df-4e1b-9994-e529afecaf54).
- Source snapshot:
  `https://ies-ows.jrc.ec.europa.eu/iforce/gfc2020/wms.py?SERVICE=WMS&VERSION=1.3.0&REQUEST=GetMap&LAYERS=gfc2020_v3&STYLES=&CRS=EPSG:4326&BBOX=-90,-180,90,180&WIDTH=720&HEIGHT=360&FORMAT=image/png&TRANSPARENT=TRUE`
- Retrieved: 2026-08-21. Source PNG: 13,277 bytes. SHA-256:
  `04c0faee578a370537cb28f6a99957bdd2de880fdc09d7d6eb069905f2dbc7c6`.
- Reuse: the [European Commission legal
  notice](https://commission.europa.eu/legal-notice_en) permits reuse with
  appropriate credit and an indication of changes under CC BY 4.0.
- Changes: requested from the official WMS as a 720 x 360 transparent
  EPSG:4326 display snapshot. Its forest alpha mask was losslessly repacked
  into a categorical byte mask and recoloured by Minerals; the source's
  forest/non-forest classification was not changed.
- Limitation: JRC describes this WMS RGB output as visualization-only and not
  suitable for analysis.

Display credit: Forest data © European Union, 2026 — JRC Global Forest Cover
2020 v3 (modified for display), DOI 10.2905/JRC.354CG88.

## World land outline

- Source: Natural Earth `ne_110m_land`, project release v5.1.2:
  `https://raw.githubusercontent.com/nvkelso/natural-earth-vector/v5.1.2/geojson/ne_110m_land.geojson`
- Source GeoJSON: 138,160 bytes. SHA-256:
  `9e0729ee253ca7d7a5c4ae9395fb1902264c5377c52e224d13dd85010e2835d9`.
- Reuse: public domain; see the [Natural Earth terms of
  use](https://www.naturalearthdata.com/about/terms-of-use/).
- Changes: properties were removed and geometry was rasterized to the same
  720 x 360 categorical land mask; styling was added by Minerals.

Optional display credit: Made with Natural Earth.
