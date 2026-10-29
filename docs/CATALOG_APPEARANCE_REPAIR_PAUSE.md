# Catalog appearance repair, paused 2026-10-29

The private catalog database is not in git. These notes are the resume point for the bad descriptions written by the fast catalog loops.

## What went wrong

Workers closed catalog jobs when a PDF timed out, returned 403, or was only an abstract. The public line became “was first described from [country]” and the job was marked complete. A check that only confirmed the database write treated that as success.

A country sentence is allowed only when the cited paper itself gives neither color nor habit. An unopened paper leaves the record unfinished.

Do not publish the stored country value “unknown”. Do not put review notes in the public description. Do not restart the four fast loops (`catalog-appearance-loop` through `catalog-appearance-loop-4`).

## Still wrong on the private database

- Augite: “Augite was first described from unknown.”
- Baryte: “Baryte was first described from an unknown country.”

The cited pages that could be opened do not name a discovery place. Leave both lines until a cited paper does.

## Corrected from opened pages

American Mineralogist 91 (2006) 1249 and 1260, open first pages only. Those pages state no color.

- Arrojadite-(KNa): holotype at Rapid Creek, Yukon Territory, Canada, crystals in hydrothermal veins cutting shale and ironstone.
- Arrojadite-(SrFe): holotype at Horrsjöberg, Värmland, Sweden.
- Arrojadite-(KFe): Nickel Plate mine sample from Keystone, South Dakota.

## Country sentences closed without the paper

Re-read the cited paper before changing these. Do not close them again as a country sentence if the paper is still unopened.

Aeschynite-(Ce), Ahrensite, Aktashite, Allactite, Allargentum, Alluaudite, Aluminoceladonite, Andalusite, Andyrobertsite, Anhydrite, Ankerite, Antimony, Ardealite, Argentopentlandite, Argentopyrite, Arsenic, Arsenolite, Arsenopalladinite, Arupite, Asbecasite, Astrophyllite, Atelestite, Auricupride, Autunite, Averievite, Avicennite, Avogadrite, Axinite-(Mg), Axinite-(Mn), Azoproite, Babingtonite, Bannisterite, Baotite, Barentsite, Bario-olgite, Bariolakargiite, Barringerite, Barylite, Bassanite, Bastnäsite-(Ce), Bastnäsite-(La), Baumhauerite II, Bavenite, Bayerite, Baylissite, Becquerelite, Benjaminite.

Also re-check these place or country lines; the unread flag was not set: Antipovite, Bacaferrite, Bahariyaite, Barkovite, Beershevaite.

## Queue holds

Last catalog content-queue count: 352 complete, 4 in progress, about 3,877 pending, of 4,233. Still reserved when the workers were killed:

- Bergenite, worker `catalog-appearance-loop-3`
- Bergslagite, worker `catalog-appearance-loop-4`
- Berlinite, worker `catalog-appearance-loop`
- Bermanite, worker `catalog-appearance-loop-2`

Reconcile those claims before calling `content-queue next`. Manual description fixes use worker `catalog-appearance-fix` and `next(worker, 'rewrite', public_id)`.

Nothing from this pass was exported. The public site is still commit `23d43e6`.
