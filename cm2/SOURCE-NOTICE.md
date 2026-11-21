# Source and third-party notice

The native `.mzdash` file is editable JSON, including the dashboard's telemetry expressions and layout. It is the source distributed for editing this public dashboard. `customize.cjs` provides an additional source-level personalization path.

The earlier authoring workflow used the `UI/DjsonImport` pipeline from **AZOM**, a GPL-3.0 project by giantorth and contributors: <https://github.com/giantorth/AZOM>. The conversion pipeline generated native MOZA structures and method-chain bindings. The public dashboard and accompanying scripts are distributed under GPL-3.0 to keep that authoring provenance explicit. The GPL text is included as `LICENSE`.

Paddock layout, telemetry expressions, public native-shape branding, scripts and demo composition: Paddock contributors, 2026. Changes in this public edition replace all personal name, racing-team logo, driver-number artwork and signature artwork with editable Paddock shapes/text. No Red Bull or Max Verstappen image assets are included.

MOZA Dashboard Studio was used locally to render the demonstration images. Its proprietary program, fonts and renderer binaries are not included. The dashboard references the native `Roboto` font family by name; no font file is embedded in this package.

The AZOM plugin, SimHub and DNR lap-history provider are not bundled. The mapping formulas refer to the optional SimHub property `DNRLEDs.Data.Last7LapsData` and use AZOM channels as transport slots.

The Paddock application in `../app` is independent of the dashboard authoring pipeline and is covered by its own license and dependency notices. This notice does not relicense any third-party project.
