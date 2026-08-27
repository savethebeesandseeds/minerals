# Minerals project instructions

## One canonical annotatable development page

During active visual development, `http://127.0.0.1:18965/` is the only public
web entry and the real application must be directly selectable by Codex's
annotation control.

- Do not create a separate review HTML document, selector-session URL, helper
  server, or alternate rendering path.
- Keep the canonical `index.html` meta policy and local Nginx response aligned
  with the narrow `style-src-elem 'self' 'unsafe-inline'` exception required by
  the annotation overlay.
- Keep inline scripts blocked and retain `style-src-attr 'none'`.
- Leave `deploy/nginx/minerals-static.conf` strict. Harden the canonical HTML
  policy only when the user says visual development is complete and the final
  production-security pass begins.
