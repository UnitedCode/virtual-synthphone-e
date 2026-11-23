# The photographed instrument

The desktop UI uses the owner's two Synthphone photographs. Preserve its real keys,
plastic, dust, uneven labels, handwriting, wear and proportions. The photos are
the instrument's visible surface; CSS must not draw replacement keys or a shell.

The built-in image editing tool produced three transparent layers:

- `phone-on-hook.png`: remove the photographed room, keep the complete phone.
- `phone-base.png`: remove only the receiver and uncover the cradle, keeping
  the base at the same coordinates.
- `receiver.png`: isolate only the receiver, in its original canvas position.

The prompts explicitly required photographic identity, original writing and
wear, genuine alpha, unchanged perspective, and no redesign or studio rendering.
The base and receiver use matching 1306×1205 canvases; the source cutout is
1305×1205 and is normalized by CSS. WebP files are lossless `cwebp -lossless
-exact -m 6` encodings of these PNGs. `wood.webp` is a lossless copy of the
existing K2 viewer's `assets/keyboard/wood-grain.png` texture.

`src/photo_geometry.rs` places small, accessible hit areas over the photographic
keycaps and encoder. Pressed keys show those same photo pixels, darkened and
slightly depressed. The OLED canvas occupies the photographed display slot.

`keypad-closeup.webp` is the owner's front-on keypad photograph, cropped to
1150×1670 for the original mobile layout. `keypad-restored.webp` is an
1088×1445 WebP made with the built-in imagegen tool, using the owner's original
photo as the reference. The mobile hit areas and live OLED are positioned
against the regenerated panel in `phone/phone.css`.

<details><summary>Mobile keypad image prompt</summary>

> Edit the supplied reference photo into a clean, believable macro product photograph of the same red Synthphone E telephone keypad, for the touch surface of an interactive instrument. Use the original photo as the reference for the red handset plastic, square smoke-gray keycaps, inset key wells, and hardware proportions. Camera perfectly square and straight overhead so every edge is level and the touch targets can be aligned to the pixels. Portrait crop around the real panel, with the molded rounded red outer edges visible close to the frame; the phone fills the frame. Use a warm deep cherry-red satin ABS surface with very fine molding grain and a few faint real handling marks. Replace the loud triangular scuffs and phone-camera glare with even soft workbench daylight, soft authentic contact shadows and sharply legible legends. Preserve the old Telephone Company keypad character, recessed near-black key wells and squared beveled hard-plastic keycaps with a little patina. Exactly 12 keys in 4 rows of 3, evenly placed, with the physical off-white legends as follows, letters above the number: row 1 “1”, “ABC” over “2”, “DEF” over “3”; row 2 “GHI” over “4”, “JKL” over “5”, “MNO” over “6”; row 3 “PRS” over “7”, “TUV” over “8”, “WXY” over “9”; row 4 “*”, “0”, “#”. Use a single small inactive indicator lens centered above the keypad and a single blank, dark smoked-glass, horizontal 4:1 OLED window centered below it, roughly the width of two keys side-by-side, positioned close to the bottom of the usable plate with a small red lip below. The red housing extends nearly to the image edges. Real secondhand vintage lab hardware photographed with a normal macro lens and a single diffuse light, balanced exposure, even legibility. No other controls, text, marks, logo, decorative symbols, cast glares, lit screen contents, dramatic studio lighting, exaggerated bevels, futuristic details, digital render look, or added parts.

</details>
