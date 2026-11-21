const fs=require('fs'),path=require('path');
const root=path.resolve(__dirname,'..');
const version=fs.readFileSync(path.join(root,'Cargo.toml'),'utf8').match(/^version = "([^"]+)"/m)[1];
let text=`Paddock ${version} - dependency source license notices\n\n`;
function licenseFiles(directory,label){
  if(!fs.existsSync(directory))return;
  for(const entry of fs.readdirSync(directory,{withFileTypes:true})){
    if(entry.isFile()&&/licen[cs]e|copying|notice|ofl|MIT|ISC/i.test(entry.name)){
      text+=`\n\n===== ${label}/${entry.name} =====\n`+fs.readFileSync(path.join(directory,entry.name),'utf8');
    }
  }
}
function nodePackages(directory){
  for(const entry of fs.readdirSync(directory,{withFileTypes:true})){
    if(!entry.isDirectory()||entry.name.startsWith('.'))continue;
    const folder=path.join(directory,entry.name);
    if(entry.name.startsWith('@')){nodePackages(folder);continue;}
    if(fs.existsSync(path.join(folder,'package.json'))){const pkg=JSON.parse(fs.readFileSync(path.join(folder,'package.json'),'utf8'));licenseFiles(folder,`${pkg.name}@${pkg.version}`);}
    if(fs.existsSync(path.join(folder,'node_modules')))nodePackages(path.join(folder,'node_modules'));
  }
}
nodePackages(path.join(root,'frontend','node_modules'));
const cargoHome=fs.existsSync(path.join(root,'.dev','cargo'))?path.join(root,'.dev','cargo'):(process.env.CARGO_HOME||path.join(require('os').homedir(),'.cargo'));
const registry=path.join(cargoHome,'registry','src');
if(fs.existsSync(registry)){
  const sources=fs.readdirSync(registry).map(name=>path.join(registry,name));
  const lock=fs.readFileSync(path.join(root,'Cargo.lock'),'utf8');
  for(const match of lock.matchAll(/\[\[package\]\]\s+name = "([^"]+)"\s+version = "([^"]+)"/g)){
    const name=`${match[1]}-${match[2]}`;for(const source of sources)licenseFiles(path.join(source,name),name);
  }
}
licenseFiles(path.join(root,'assets','framework','licenses'),'Paddock bundled UI assets');
licenseFiles(path.join(root,'assets','webview2'),'Microsoft WebView2 SDK 1.0.3800.47');
fs.writeFileSync(path.join(root,'docs',`DEPENDENCY-NOTICES-${version}.txt`),text);
console.log(`Wrote dependency notices (${text.length} characters)`);
