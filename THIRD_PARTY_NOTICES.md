# Third-party notices

The app and website's original code use MIT. The separate CM2 package uses GPL-3.0; its editable .mzdash is provided as source. A license does not grant rights in third-party trademarks.

| Component | License / notice | Scope |
|---|---|---|
| React / React DOM | MIT | App frontend |
| [Astryx](https://github.com/meta/astryx) | MIT | App UI library |
| [StyleX](https://github.com/facebook/stylex) | MIT | App styling runtime/tooling |
| Lucide / Feather | ISC / MIT | Icons |
| Figtree / Pretendard | SIL OFL 1.1 | App/site fonts; names reserved by their authors |
| WebView2 loader | Microsoft BSD-style license in app/assets/webview2 | Windows runtime loader |
| Rust crates | Respective upstream licenses | See app/docs/DEPENDENCY-NOTICES-1.0.0.txt |
| [AZOM](https://github.com/giantorth/AZOM) dashboard tooling | GPL-3.0 | Separate cm2/ materials; see cm2/SOURCE-NOTICE.md |

Full bundled notices live in app/assets/framework/licenses, app/assets/webview2, app/docs/THIRD_PARTY_NOTICES.txt and app/docs/DEPENDENCY-NOTICES-1.0.0.txt. Compiled distributions include them in licenses/. The website self-hosts Figtree and includes docs/media/FIGTREE-LICENSE.txt.

The public dashboard uses native Paddock shapes and editable generic driver text. No Red Bull/Max Verstappen artwork, signatures, personal driver artwork, official iRacing photographs, full official schedule dataset, MOZA Studio binaries, AZOM binaries or DNR binaries are redistributed here. The test schedules are hand-authored synthetic fixtures. Companion app names and circuit/car names identify compatibility and examples. Refresh downloads the user's schedule from iRacing at runtime.

The use of a GPL tool alone does not automatically license all output under GPL; this repository conservatively licenses its separate CM2 materials under GPL-3.0 because the conversion path also uses AZOM-generated scripting material. See the source notice for the exact boundary.
