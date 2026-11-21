// The paddock theme. `astryx experimental theme editor` wrote this file once and never changes it again.
// The checked settings live in paddock.settings.json and the generated paddock.settings.ts;
// change them with `astryx experimental theme editor set paddock ...`. Add your own changes below: they win over the settings.
import {defineTheme} from '@astryxdesign/core/theme';
import {paddockSettings} from './paddock.settings';

export const paddockTheme = defineTheme({
  extends: paddockSettings,
  name: 'paddock',
});
