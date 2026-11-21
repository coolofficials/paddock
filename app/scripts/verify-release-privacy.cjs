const fs = require('node:fs');
const path = require('node:path');
const directory = path.resolve(process.argv[2] || path.join(__dirname, '..', 'target', 'release'));
const findings = [];
let checked = 0;
function scan(folder) {
  for (const entry of fs.readdirSync(folder, { withFileTypes: true })) {
    const file = path.join(folder, entry.name);
    if (entry.isDirectory()) continue;
    if (!/\.(exe|dll)$/i.test(entry.name)) continue;
    checked++;
    const bytes = fs.readFileSync(file);
    for (const encoding of ['utf8', 'utf16le']) {
      const content = bytes.toString(encoding);
      // Virtual rustc /rustc/<hash>, /rust-deps and /paddock paths are safe.
      const patterns = [
        ['Windows home path', /[A-Z]:[\\/]+Users[\\/]+[^\\/\s\0"<>]+/i],
        ['POSIX home path', /\/(?:Users|home)\/[^/\s\0"<>]+/],
      ];
      for (const [kind, pattern] of patterns) {
        if (pattern.test(content)) findings.push({ file: entry.name, encoding, kind });
      }
    }
  }
}
scan(directory);
if (!checked) throw new Error('No release executables or DLLs found.');
if (findings.length) {
  // Never print the private path itself into build logs.
  console.error(JSON.stringify({ passed: false, checked, findings }));
  process.exitCode = 1;
} else {
  console.log(JSON.stringify({ passed: true, checked, localHomePaths: 0 }));
}
