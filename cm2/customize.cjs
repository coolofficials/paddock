#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 Paddock contributors
const fs=require('node:fs'),path=require('node:path');
const options={};
for(let i=2;i<process.argv.length;i+=2){const key=process.argv[i];if(!['--name','--short-name','--number','--output'].includes(key)||!process.argv[i+1])throw Error('Usage: node customize.cjs --name "ALEX DRIVER" --short-name "ALEX" --number "27" --output "My Paddock CM2.mzdash"');options[key]=process.argv[i+1];}
const name=options['--name']||'YOUR NAME',short=options['--short-name']||'DRIVER',number=options['--number']||'01';
if(name.length>24||short.length>12||!/^\d{1,3}$/.test(number))throw Error('Use a name up to 24 characters, a short name up to 12, and a 1–3 digit number.');
const source=path.join(__dirname,'Paddock CM2.mzdash'),output=path.resolve(options['--output']||'My Paddock CM2.mzdash');
if(output.toLowerCase()===source.toLowerCase())throw Error('Choose a different output path to preserve the template.');
const dash=JSON.parse(fs.readFileSync(source,'utf8'));
function visit(n){if(n.name==='Short driver name')n.text.text=short;if(n.name==='Short idle / Driver name')n.text.text=name;if(n.name==='Short driver number'||n.name==='Short idle / Driver number')n.text.text=number;for(const c of n.children||[])visit(c);}visit(dash);
fs.writeFileSync(output,JSON.stringify(dash,null,2)+'\n');console.log('Created '+output);
