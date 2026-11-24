# D101 Label Studio user guide

## Getting started and choosing a language

Extract the Windows 11 x64 release into a folder and run niimbot-d101.exe. The graphical editor requires Windows App Runtime 2 x64. On first launch you will see a project with one label.

Open Settings → Language / Язык (Настройки → Язык / Language in Russian), choose English or Русский. Menus, dialogs and this guide switch immediately. The app remembers the choice on the next launch. Text, block content and label names you enter stay in the project's language.

Open Help from the top bar. Type one or more words such as “cable” or “RFID usage”. The topic list includes sections containing every search word. Select a topic to read it. Clear the search box to see the full contents.

## Connecting over Bluetooth

Turn on your D101 and enable Bluetooth in Windows. Disconnect the printer from the phone app if it is using the device. Open Settings → Bluetooth printer… → Find Niimbot D101, select your device and choose Save. Check the model and connection state in the top strip.

Leave Protocol set to Automatic. Choose B1 or V4 manually if the app cannot identify the protocol reply. Status polling and printing share the Bluetooth connection. The app periodically tries to reconnect after a disconnect. Select and save a different device in the same dialog to switch printers.

## Battery, RFID and remaining labels

The status strip shows the model, battery, label type and remaining quantity. The vertical battery icon reflects the reported charge. Use the circular arrow to refresh. While idle, the app reads the printer about every 30 seconds and again after printing.

The printer's RFID reading supplies usage. The app subtracts usage from roll capacity. When firmware omits capacity, the app looks up the product code in the NIIMBOT catalog. Product metadata is cached locally. Wait until a running print job finishes to see the updated count. If the type or capacity is unknown, the app displays “No data”.

## Stock sizes, outlines and cable labels

Open Settings → Label size… and select the stock from the list. The first dimension in the dialog is the width across the roll, the second is the feed length. The long side is horizontal on the canvas. When you connect or change a recognised roll, you can apply its size and shape to every label in the project. Check block positions after shrinking the canvas. Edit → Undo reverses the change.

Stock default selects the standard outline. You can also choose a rectangle, rounded corners or an oval. If a roll-specific outline is available, it follows the die cut. Art outside the outline is clipped in print and PNG exports.

On a 109 × 12.5 mm cable label the first 35 mm on the left is the narrow wrap. Place artwork for the two flag faces at 35–72 mm and 72–109 mm. The dotted line at 72 mm marks the fold. On 25 mm stock, grey margins indicate the area outside the printhead.

## Creating a project and arranging layers

Choose File → New project to start a set. The square sheet-and-plus button in the left column adds a label. Give it a name, set the copy count and select Include in set to print it with the other labels.

Use Insert to add text, an image, a rectangle or a line. Select a block on the canvas or in the layers list. Drag to move it and use the eight handles to resize its frame. Grid, Snap to 0.5 mm and Fit to window help with placement. Edit contains duplication, deletion, stacking order, Undo and Redo.

## Text, fonts and alignment

Select a text block and enter its content in the right column. Choose an installed Windows font and a size in points. The font style buttons toggle bold, italic, underline and strikethrough. Use the small arrows beside the font size to adjust it.

In Text alignment, the upper row positions the text horizontally and the lower row vertically. In Align block to label the corresponding controls move the entire frame on the label. Increase the frame size or reduce the font size if longer text is clipped. Resizing the frame preserves the selected font size.

## Images and shapes

Use Insert → Image… to select a PNG or JPEG. The project embeds the image. The frame sets its available space and the image keeps its proportions. Adjust Black threshold to choose which pixels print in black. Raising it turns more shades into black.

Set the outline thickness for a rectangle and enable Fill when needed. A line has a thickness setting. Open Print preview to check the visible portions of all blocks before printing.

## Generating labels from a range or list

Create a text block containing “Cable {number}”. Open Insert → Label generator… and select the variable. For a number range, enter 1 for From, 10 for To and 1 for Step. Set Minimum digits to 3 to produce “Cable 001” through “Cable 010”. The end value is included when a step lands on it. Use a negative step for a descending range.

For a list, select List of strings and enter one value per line. Empty lines are skipped. A variable can be used more than once and in multiple blocks. To print literal braces, type {{ and }}.

For multiple variables, Match by row pairs corresponding entries. A single value can be reused for all rows. All combinations creates a label for every combination. Check the sample and copy count, then select Create set. The template stays in the project, excluded from printing. Undo removes the entire generated set in one operation.

## Preview, batch printing and the queue

Choose View → Print preview and page through the included labels. Print current label sends the selected label with its copy count. Print set sends all included labels in order. A job can contain up to 1,000 copies. Adjacent identical copies are sent together.

Open Print → Print queue to see progress or cancel an active job. If the connection drops, check which labels actually came out and resubmit only the missing ones. Interrupted jobs are not automatically replayed.

## Density, rotation and print calibration

Set density from 1 to 3 and choose the media type in Bluetooth printer settings. To change orientation, enable Rotate print by 180° in the label size dialog. Print preview shows the selected orientation.

If the entire design prints too high or too low, open Settings → Print calibration… and adjust the offset one dot at a time. At 203 dpi one dot is approximately 0.125 mm. Choose Down one dot if the print is too high. Save and check the next label. The offset is stored for the selected printer and applies to new editor and Windows jobs. Set it to zero to restore the original position.

## Saving projects, CSV import and PNG export

File → Save… writes a JSON project containing labels, blocks, embedded images and generator settings. File → Open… restores it. A local draft is also saved automatically. To move the project between computers, take the explicitly saved file and install the same fonts.

For CSV import use UTF-8, a comma separator and the header text,copies. A sample row is USB cable,2. Enclose fields containing a comma or line break in double quotes. Each record creates a label. File → Export PNG… saves the selected label at 203 dpi with transparent edges following the outline.

## Printing from Word, PDF viewers and other Windows apps

Keep the editor running. In the extracted release folder, run scripts/Install-Printer.ps1 from an administrator PowerShell session. After registration, select Niimbot D101 in the application's print dialog. Choose the roll width, label length and number of copies. Open PDFs in a viewer and use its normal Print command.

Windows converts the document to PWG Raster and passes it to the local IPP bridge. You can minimise the editor. Closing it stops the bridge. To remove the system queue, run scripts/Uninstall-Printer.ps1 as administrator.

## Troubleshooting

If discovery finds no printer, check power, Windows Bluetooth and the phone connection. If the status shows Offline, move the printer closer and refresh. If the port or data folder is in use, check for a second editor or headless bridge.

For clipped text, check the frame and font size. If the whole label is clipped, compare the roll width and the printhead margins. On cable stock, check both flag positions. If the entire image has a consistent vertical shift, use print calibration.

To test without a printer, run niimbot-d101.exe --simulate. Simulation uses a separate profile and saves job images under its simulated folder. Use a physical test print to verify placement on the roll.
