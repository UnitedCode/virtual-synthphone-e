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

`mobile-handset.webp`, `mobile-encoder.webp`, and `mobile-mic-knob.webp` use
pixels from the owner's second photograph. The parts were cropped, masked, and
leveled for the mobile controls.

`mobile-encoder-wheel.webp` shows the encoder face-on. The built-in imagegen
tool used the photographed side view as a material reference. The pale index
mark shows each turn as the wheel sends encoder steps to the firmware.

<details><summary>Encoder wheel image prompt</summary>

> Make a single small photorealistic product-photo asset for this Synthphone E rotary encoder wheel seen face-on. The attached image is an actual side view of the existing knob and is the material reference: plain short black injection-molded plastic cap with straight grip flutes. Show it dead-on from the axis, perfectly circular, centered, occupying about 75% of a square canvas; visible dark grip ribs all around the rim and a flat satin black center with a very slight circular molding grain. Include one tiny faded ivory index nick near the rim at twelve o'clock so the player can see it turning. Use soft even window light, a slight genuine specular edge and a little real handling wear. It should look like a real inexpensive workshop electronics knob photographed with a macro lens. Isolate the knob on true transparent alpha, no other objects, no count marks, text, ring markings, logos, reflections of anything, colorful LEDs, metallic silver accents, oversized bevels, complicated machinery, digital rendering or background.

</details>

<details><summary>Mobile keypad image prompt</summary>

> Edit the supplied reference photo into a clean, believable macro product photograph of the same red Synthphone E telephone keypad, for the touch surface of an interactive instrument. Use the original photo as the reference for the red handset plastic, square smoke-gray keycaps, inset key wells, and hardware proportions. Camera perfectly square and straight overhead so every edge is level and the touch targets can be aligned to the pixels. Portrait crop around the real panel, with the molded rounded red outer edges visible close to the frame; the phone fills the frame. Use a warm deep cherry-red satin ABS surface with very fine molding grain and a few faint real handling marks. Replace the loud triangular scuffs and phone-camera glare with even soft workbench daylight, soft authentic contact shadows and sharply legible legends. Preserve the old Telephone Company keypad character, recessed near-black key wells and squared beveled hard-plastic keycaps with a little patina. Exactly 12 keys in 4 rows of 3, evenly placed, with the physical off-white legends as follows, letters above the number: row 1 “1”, “ABC” over “2”, “DEF” over “3”; row 2 “GHI” over “4”, “JKL” over “5”, “MNO” over “6”; row 3 “PRS” over “7”, “TUV” over “8”, “WXY” over “9”; row 4 “*”, “0”, “#”. Use a single small inactive indicator lens centered above the keypad and a single blank, dark smoked-glass, horizontal 4:1 OLED window centered below it, roughly the width of two keys side-by-side, positioned close to the bottom of the usable plate with a small red lip below. The red housing extends nearly to the image edges. Real secondhand vintage lab hardware photographed with a normal macro lens and a single diffuse light, balanced exposure, even legibility. No other controls, text, marks, logo, decorative symbols, cast glares, lit screen contents, dramatic studio lighting, exaggerated bevels, futuristic details, digital render look, or added parts.

</details>
