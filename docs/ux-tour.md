# Picsie UX tour: five flows

These are direct screenshots of the packaged Linux QuickGUI app, captured on 2026-09-23. Flows 1–4 were captured at 1280 × 900 immediately before the project-picker fix; flow 5 shows the added **Open .comp** action. Flow 1 uses a new document; flows 2–3 use the included sample; flows 4–5 start fresh from that sample.

You can also inspect the [sample project](ux-tour-sample.picsie) and its [PNG export](ux-tour-sample.png).

## 1. Start a canvas and add content

1. **Click New.** The dialog offers a project name and canvas dimensions. The default is a 1200 × 800 transparent canvas.

   ![New canvas dialog](screenshots/ux-tour/01-01-new-dialog.png)

2. **Click Create.** A separate editor window opens with a checkerboard canvas and an empty layer panel.

   ![Empty transparent canvas](screenshots/ux-tour/01-02-blank-canvas.png)

3. **Choose the coral swatch and Rectangle tool, then drag on the canvas.** The new shape appears as its own selected layer.

   ![Coral rectangle and layer row](screenshots/ux-tour/01-03-rectangle.png)

4. **Choose the blue swatch and Ellipse tool, then drag again.** The ellipse appears above the rectangle in the layer stack.

   ![Blue ellipse above the rectangle](screenshots/ux-tour/01-04-ellipse.png)

5. **Click with the Text tool.** Picsie adds a text layer; scroll the inspector to edit its text, size, and font. This capture still shows the default “Your text” content.

   ![Text layer and its inspector](screenshots/ux-tour/01-05-text-layer.png)

## 2. Select, group, and move layers

1. **Start from the included sample.** The left rail contains tools, the center is the canvas, and the right side holds color, layers, and properties.

   ![Included sample composition](screenshots/ux-tour/02-01-sample.png)

2. **Select Coral orbit, then Shift-click Electric blue.** Both layer rows and their canvas bounds are selected.

   ![Two layers selected](screenshots/ux-tour/02-02-multiselect.png)

3. **Click Group.** A new folder contains both circles and can be collapsed in the layer panel.

   ![New folder containing the circles](screenshots/ux-tour/02-03-group.png)

4. **Select Coral orbit and drag it on the canvas.** Its transform handles and the composition update together.

   ![Coral orbit moved on the canvas](screenshots/ux-tour/02-04-move.png)

## 3. Paint a layer mask

1. **Select Electric blue and scroll to Layer Mask** in the inspector.

   ![Electric blue selected before adding a mask](screenshots/ux-tour/03-01-select-layer.png)

2. **Click Add mask.** The toolbar switches to Brush with Hide and Reveal modes; the inspector shows mask controls.

   ![Mask added and controls visible](screenshots/ux-tour/03-02-add-mask.png)

3. **Paint with Hide.** The stroke conceals part of the blue layer, revealing the background beneath it.

   ![Hidden section of Electric blue](screenshots/ux-tour/03-03-hide.png)

4. **Switch to Reveal and paint across part of the stroke.** Blue pixels return where the reveal stroke passes.

   ![Part of the blue layer revealed again](screenshots/ux-tour/03-04-reveal.png)

5. **Click Disable.** The full blue circle reappears while the mask remains attached for later editing.

   ![Disabled mask restores the original layer](screenshots/ux-tour/03-05-disable.png)

## 4. Expand the canvas

1. **Click the dimensions in the bottom-left footer** to open Canvas Size. Choose the top-left anchor to keep that corner fixed.

   ![Canvas Size dialog and top-left anchor](screenshots/ux-tour/04-01-canvas-size.png)

2. **Enter 1440 × 960.** The dialog previews the new dimensions before applying them.

   ![Canvas size set to 1440 by 960](screenshots/ux-tour/04-02-dimensions.png)

3. **Open Canvas extension.** The picker offers transparent, foreground, black, white, and custom fills.

   ![Canvas extension choices](screenshots/ux-tour/04-03-extension-options.png)

4. **Choose White.** The selected extension appears in the form before applying.

   ![White extension selected](screenshots/ux-tour/04-04-white-selected.png)

5. **Click Resize canvas.** The artwork keeps its scale; white space is added on the right and bottom as a separate bottom layer.

   ![Expanded canvas with white extension](screenshots/ux-tour/04-05-expanded.png)

## 5. Save, export, and reopen

1. **Click Save.** The native dialog proposes a `.picsie` project file.

   ![Native project save dialog](screenshots/ux-tour/05-01-save-dialog.png)

2. **Save the project.** The editor returns with a “Saved” message. The [saved sample project](ux-tour-sample.picsie) remains editable.

   ![Editor after saving the project](screenshots/ux-tour/05-02-saved.png)

3. **Click Export PNG.** A second native dialog proposes a `.png` file.

   ![Native PNG export dialog](screenshots/ux-tour/05-03-export-dialog.png)

4. **Save the PNG.** The editor confirms the 1200 × 800 export. Here is the [exported artwork](ux-tour-sample.png).

   ![Editor after PNG export](screenshots/ux-tour/05-04-exported.png)

5. **Click Open and select the saved `.picsie` file.** The native picker now opens successfully on Linux.

   ![Native project open dialog](screenshots/ux-tour/05-05-open-file-dialog.png)

6. **Click Select.** Picsie opens the project in a second editor window. The front window is resized and fitted here so both windows are visible.

   ![Saved project reopened in a second window](screenshots/ux-tour/05-06-reopened.png)

7. **For a Compositor package, use Save .comp and Open .comp.** The folder picker lets you enter the `.comp` directory and choose it.

   ![Folder picker inside a Compositor package](screenshots/ux-tour/05-07-comp-picker.png)

8. **Click Select to reopen the package.** This 960 × 640 capture also shows the toolbar with both Open actions at the minimum window size. The `.comp` export rasterizes live text, shapes, and gradients.

   ![Compositor package reopened in a compact window](screenshots/ux-tour/05-08-comp-reopened.png)
