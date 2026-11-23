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
leveled for an earlier mobile layout.

`mobile-encoder-wheel.webp` shows the encoder face-on. The built-in imagegen
tool used the photographed side view as a material reference. The encoder and
mic controls use the same wheel at different sizes. Their index marks show
encoder steps and mic level.

`mobile-phone-face.webp` uses the owner's close-up photo for the mobile case,
handwritten labels, keypad, and display surround. The built-in imagegen tool
reconstructed only the cradle area hidden behind the receiver. The moving
`mobile-receiver-lift.webp` layer uses the receiver pixels from the same photo.

`mobile-original-photo.webp` uses the owner's overhead photo, cropped to
680×700 from (340, 880), with adjusted contrast and saturation. Mobile zoom
positions the real keypad and OLED across the viewport. The existing rotary
caps sit on the photographed face beside the OLED.

`mobile-original-receiver.webp` masks the handset from those same photo pixels
so it can follow the player's finger. `mobile-original-cradle.webp` blends in the
upper patch from the built-in image edit while the handset is moving or lifted.
The on-hook image uses the original photo across the entire instrument.

<details><summary>Overhead cradle patch image prompt</summary>

> This attached photo is the edit target for an empty-cradle patch. Remove only the horizontal red handset at the top (both ends and the handle that links them) and its short wire coming in from the left. Expose the molded red cradle underneath it, with the existing two square red supports continuing up to meet the back edge. Continue the beige tabletop behind either end where appropriate. Keep the cradle supports which are already visible below the handset in exactly the same positions, match the original imperfect warm photograph and its cherry red plastic. Keep the masking tape, handwritten words, keypad, OLED window, base, perspective, and all remaining pixels exactly where they are. No zoom, no reframing, no text changes, no extra controls, no graphic redesign. Only the pixels previously obscured by the handset will be used.

</details>

<details><summary>Empty cradle image prompt</summary>

> Photographic inpainting for an interactive Synthphone E, using the attached real front-on phone photo crop. Remove only the bright horizontal red handset that crosses the middle of the picture from its left edge to its right edge. Reveal the actual-looking stationary cherry-red handset cradle beneath, with the two existing rectangular molded red lifting supports at x≈35–107 and x≈389–464 extending upward just enough that the handset could rest on them. The back of the cradle is the original shallow dark burgundy strip already visible along the top of this reference. Match exact framing, close camera angle, color, faint blur, shine, original physical molding, and uneven warm light of this real photo. Keep the bottom part of both cradle supports and surrounding red base at their existing positions and keep the narrow background along the top. The new plastic between them is a believable recessed cradle surface, plain and moderately dark red with natural shadow. Use the exact same landscape crop, no resize or zoom, no rotation, no new knobs or other parts, no writing or decals, no extra silhouettes or invented holes, no studio restyling. This is one matched empty-cradle frame of the same photographed object immediately after the receiver was pulled away.

</details>

<details><summary>Encoder wheel image prompt</summary>

> Make a single small photorealistic product-photo asset for this Synthphone E rotary encoder wheel seen face-on. The attached image is an actual side view of the existing knob and is the material reference: plain short black injection-molded plastic cap with straight grip flutes. Show it dead-on from the axis, perfectly circular, centered, occupying about 75% of a square canvas; visible dark grip ribs all around the rim and a flat satin black center with a very slight circular molding grain. Include one tiny faded ivory index nick near the rim at twelve o'clock so the player can see it turning. Use soft even window light, a slight genuine specular edge and a little real handling wear. It should look like a real inexpensive workshop electronics knob photographed with a macro lens. Isolate the knob on true transparent alpha, no other objects, no count marks, text, ring markings, logos, reflections of anything, colorful LEDs, metallic silver accents, oversized bevels, complicated machinery, digital rendering or background.

</details>

<details><summary>Mobile keypad image prompt</summary>

> Edit the supplied reference photo into a clean, believable macro product photograph of the same red Synthphone E telephone keypad, for the touch surface of an interactive instrument. Use the original photo as the reference for the red handset plastic, square smoke-gray keycaps, inset key wells, and hardware proportions. Camera perfectly square and straight overhead so every edge is level and the touch targets can be aligned to the pixels. Portrait crop around the real panel, with the molded rounded red outer edges visible close to the frame; the phone fills the frame. Use a warm deep cherry-red satin ABS surface with very fine molding grain and a few faint real handling marks. Replace the loud triangular scuffs and phone-camera glare with even soft workbench daylight, soft authentic contact shadows and sharply legible legends. Preserve the old Telephone Company keypad character, recessed near-black key wells and squared beveled hard-plastic keycaps with a little patina. Exactly 12 keys in 4 rows of 3, evenly placed, with the physical off-white legends as follows, letters above the number: row 1 “1”, “ABC” over “2”, “DEF” over “3”; row 2 “GHI” over “4”, “JKL” over “5”, “MNO” over “6”; row 3 “PRS” over “7”, “TUV” over “8”, “WXY” over “9”; row 4 “*”, “0”, “#”. Use a single small inactive indicator lens centered above the keypad and a single blank, dark smoked-glass, horizontal 4:1 OLED window centered below it, roughly the width of two keys side-by-side, positioned close to the bottom of the usable plate with a small red lip below. The red housing extends nearly to the image edges. Real secondhand vintage lab hardware photographed with a normal macro lens and a single diffuse light, balanced exposure, even legibility. No other controls, text, marks, logo, decorative symbols, cast glares, lit screen contents, dramatic studio lighting, exaggerated bevels, futuristic details, digital render look, or added parts.

</details>
