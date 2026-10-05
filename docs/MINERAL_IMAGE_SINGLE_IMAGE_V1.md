# One image per mineral record

Use the built-in image tool directly. Save one finished image to
`data/images/<public-record-id>.png`. Get the assignment from the shared
image dispatcher described in the [agent instructions](MINERAL_IMAGE_AGENT_WORKFLOW_V1.md).
The dispatcher handles record ownership and completion in one database;
do not create per-image scripts, sidecars, manifests or folders.

Use the same setup for every record: square canvas, seamless matte pure black
background (#000000), one natural uncut specimen, generous margins, and a
three-quarter view. Light it with a broad soft neutral-white key from the upper
left and weaker front-right fill. Keep the entire specimen sharp and its edges
legible. Preserve its supported form, natural color, texture, transparency and
luster. No text, props, logos, decorative effects or extra specimens.

## Prompt

```text
Create one photorealistic studio illustration of {{mineral_name}}.
Supported appearance: {{appearance_brief}}.

Show one natural uncut specimen, centered in a square canvas with generous
margins and its longest projected dimension about 70 percent of the canvas.
Preserve the mineral's supported form, natural color, texture, transparency
and luster. Keep the whole specimen sharp.

Use a seamless matte pure black background and supporting surface (#000000),
with no horizon or texture. Use broad soft neutral-white light from the upper
left and weaker front-right fill. Keep the specimen's edges legible.

No text, labels, logos, watermark, props, extra specimens, colored lighting,
UV effects, glow or decorative sparkle. This is an AI-generated mineral
illustration, not a real specimen photograph.
```

Change only the name and supported appearance description. Use the record's
information; briefly check a mineralogical reference if appearance is uncertain.
Preserve representative powders, aggregates and microscopic forms rather than
inventing large crystals.

Call the built-in tool with `transparent_background: false`; omit reference
arguments for new images. Inspect once, copy the actual returned original file
to the record's image path, and move on. Keep the native file format and actual
returned dimensions. Only correct an obvious defect; avoid repeated framing
adjustments. Label images as AI illustrations when used in the application.

See [agent instructions](MINERAL_IMAGE_AGENT_WORKFLOW_V1.md).
