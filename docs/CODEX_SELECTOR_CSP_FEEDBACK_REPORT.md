# Codex in-app browser selector blocked by page CSP

- Date confirmed: 2026-08-25
- Status: Root cause confirmed; local review workaround implemented
- Severity: High for Codex-assisted visual review; no production-site failure
- Affected environment: Codex desktop 26.818.5229.0 (browser plugin/build 26.818.41509)
- Affected project: Minerals

## Summary

Codex's in-app browser selector could open the Minerals page, but it could not highlight or select page elements. Restarting the computer, restarting Codex, reopening the tab, and navigating away from the map did not fix it.

The map was not the site-wide cause. The page's Content Security Policy (CSP) blocked an inline `<style>` element injected by the Codex selector into its shadow root. The selector host mounted, but its interaction layer, blocker, cursor, hover box, and marker styles did not. This made the selector appear inactive even on DOM-only pages.

## User-visible symptoms

- The page rendered normally in the in-app browser.
- Selector/annotation mode did not show hover outlines or allow element selection.
- Reloading, reopening, and rebooting did not change the behavior.
- The failure affected routes without the map canvas, so it was broader than map hit testing.
- Codex could release browser control normally; this was not a stale Browser Use lock.

## Root cause

The production page defines a strict policy in both places below:

- `public-app/index.html`
- `deploy/nginx/minerals-static.conf`

The relevant directives are:

```text
style-src 'self';
style-src-attr 'none';
```

The Codex desktop selector mounts `#codex-browser-sidebar-comments-root`, creates an open shadow root, and injects a `<style>` element containing its selector UI rules. Because `style-src-elem` was not specified, it fell back to `style-src`. The inline selector stylesheet was therefore rejected by the page CSP.

This failure is deceptive: the selector's host element can exist while the visual and interactive layer it depends on is unstyled and unusable.

## Evidence that ruled out the map

- The homepage and mineral catalog contain no map canvas but showed the same selector failure under the production CSP.
- The map canvas is removed when its route is unmounted.
- A fresh, non-controlled browser tab still failed under the production entry point.
- The local review entry worked after changing only the style-element CSP allowance.

The map has a separate limitation: a canvas is one DOM element, so selector tools cannot target individual features painted inside it. Canvas pointer capture may also affect targeting within the map region. Neither behavior explains a selector failure across the rest of the site.

## Reproduction

1. Serve `public-app/index.html` with its production CSP and open it in the Codex in-app browser.
2. Enable the page selector/annotation control.
3. Attempt to hover or select ordinary DOM elements outside the map.
4. Observe that the selector host mounts but its highlights and interaction layer do not function.
5. Open `public-app/selector-review.html`, which differs by allowing inline style elements for local review.
6. Enable the selector again and confirm that DOM selection works.

## Safe local workaround

The project now has a local-only review entry:

```text
public-app/selector-review.html
```

It uses the narrow exception:

```text
style-src 'self';
style-src-elem 'self' 'unsafe-inline';
style-src-attr 'none';
```

This permits the selector's injected `<style>` element while retaining the prohibition on style attributes. Script policy remains strict. The review entry is marked `noindex` and is excluded from the production exporter.

If a server sends a CSP response header as well as a page CSP meta element, both policies apply and intersect. A local review server must therefore omit the production header or give the review response the same narrow `style-src-elem` exception.

The primary local workflow is the Compose `web` service:

```bash
docker compose up -d --no-build web
```

Open `http://127.0.0.1:18965/`. Its local-only Nginx configuration maps `/` to
the selector-safe entry, sends `Cache-Control: no-store` for the review boot
graph, and redirects an unkeyed root request to a per-process session URL. The
session URL matters because hash-only navigation does not load a new document
or reset selector state. `tools/serve-selector-review.py` remains an explicit
diagnostic fallback when Compose is unavailable; it is not the primary project
review server.

After changing the local review policy, close and reopen only the affected browser tab. Do not kill Codex, delete browser partitions, or clear global Browser Use state; those actions can disrupt unrelated projects and do not correct the page policy.

## Security constraints

- Do not weaken the production CSP to make the Codex selector work.
- Do not add `'unsafe-inline'` to `script-src`.
- Keep the exception confined to a local, non-exported review entry or local-only response.
- Keep `style-src-attr 'none'` unless a separate, verified tool requirement proves otherwise.
- Check both CSP headers and CSP meta elements before concluding that an exception is active.

## Validation

- The Minerals catalog rendered through the selector-safe review entry.
- Selector host injection completed on the review page.
- Production `index.html` retained its strict policy and contains no `'unsafe-inline'` exception.
- The review entry is absent from the public export allowlist.
- Public-app tests completed with 15 passing, 1 optional skip, and 0 failures.

## Recommended Codex product improvements

1. Make the selector overlay independent of the inspected page's CSP, for example through a browser/extension overlay or another app-controlled styling mechanism.
2. Detect failure to apply the selector stylesheet and show an explicit CSP diagnostic instead of silently presenting an inert selector.
3. Document the minimum CSP requirement and a local-review pattern that does not require weakening production policy.
4. Treat canvas targeting as a separate capability limitation and explain that painted sub-elements are not DOM-selectable.

## Recurrence checklist

When an in-app selector mounts but cannot highlight or click anything:

1. Test a DOM-only route before blaming canvas or pointer capture.
2. Inspect `style-src`, `style-src-elem`, and every CSP delivery location.
3. Check whether an injected shadow-root `<style>` was blocked.
4. Use a local-only review entry with the narrow style-element exception.
5. Reset only the affected tab and leave global Codex state intact.
