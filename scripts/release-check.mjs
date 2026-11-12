#!/usr/bin/env node
// Release contract: one version, one Windows x64 GUI application and one NSIS installer.
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
export const TARGET = 'x86_64-pc-windows-msvc';
export const BINARY = 'yeskia-app';
const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const json = p => JSON.parse(readFileSync(p, 'utf8'));
function requireValue(ok, message) { if (!ok) throw new Error(message); }
export function releasePlan(root=ROOT, expectedVersion) {
  const config=json(resolve(root,'src-tauri/tauri.conf.json'));
  const version=config.version;
  requireValue(typeof version==='string' && /^\d+\.\d+\.\d+$/.test(version),'Release version must be major.minor.patch');
  if(expectedVersion) requireValue(expectedVersion===version,`Requested version ${expectedVersion} differs from ${version}`);
  const cargo=readFileSync(resolve(root,'Cargo.toml'),'utf8').split('[workspace.package]')[1]?.split('\n[')[0];
  requireValue(/^\s*version\s*=\s*"([^"]+)"/m.exec(cargo??'')?.[1]===version,'Cargo workspace version differs from Tauri');
  for(const file of ['package.json','ui/package.json']) requireValue(json(resolve(root,file)).version===version,`${file} version differs from Tauri`);
  const lock=json(resolve(root,'package-lock.json'));
  for(const v of [lock.version,lock.packages?.['']?.version,lock.packages?.ui?.version]) requireValue(v===version,'npm lockfile versions differ from Tauri');
  requireValue(config.mainBinaryName===BINARY,'Tauri must name the application yeskia-app');
  requireValue(config.productName==='YESKIA' && config.identifier==='dev.yeskia.app','Product/installation identity changed');
  requireValue(JSON.stringify(config.bundle?.targets)===JSON.stringify(['nsis']),'Only the NSIS installer is released');
  requireValue(config.bundle?.windows?.nsis?.installMode==='currentUser','Installer must use currentUser mode');
  return {version,target:TARGET,binary:BINARY,app:`target/${TARGET}/release/${BINARY}.exe`,installer:`YESKIA_${version}_x64-setup.exe`};
}
export function checkCargoTarget(metadata, plan) {
  const app=metadata.packages.find(p=>p.name===BINARY);
  requireValue(app?.version===plan.version,'Rust executable version differs from installer');
  const bins=app?.targets.filter(t=>t.kind.includes('bin'))??[];
  requireValue(bins.length===1 && bins[0].name===BINARY && bins[0].src_path.replaceAll('\\','/').endsWith('/src-tauri/src/main.rs'),'Wrong or ambiguous application binary target');
  return true;
}
export function checkPeApplication(bytes) {
  requireValue(bytes.length>=64 && bytes.toString('ascii',0,2)==='MZ','Application is not a PE executable');
  const pe=bytes.readUInt32LE(60);
  requireValue(pe>=64 && pe+96<=bytes.length && bytes.toString('ascii',pe,pe+4)==='PE\0\0','Invalid PE header');
  requireValue(bytes.readUInt16LE(pe+4)===0x8664,'Application must be AMD64, not x86/ARM');
  requireValue(bytes.readUInt16LE(pe+24)===0x20b,'Application must be PE32+');
  requireValue(bytes.readUInt16LE(pe+24+68)===2,'Application must use Windows GUI subsystem, not a console');
  return {machine:'AMD64',format:'PE32+',subsystem:'Windows GUI'};
}
if(process.argv[1] && resolve(process.argv[1])===fileURLToPath(import.meta.url)) {
  const value=flag=>{const i=process.argv.indexOf(flag);return i<0?undefined:process.argv[i+1];};
  const plan=releasePlan(ROOT,value('--expect-version'));
  const metadata=JSON.parse(execFileSync('cargo',['metadata','--locked','--no-deps','--format-version','1'],{cwd:ROOT,encoding:'utf8'}));
  checkCargoTarget(metadata,plan);
  const app=value('--app');
  console.log(JSON.stringify({...plan,...(app?{pe:checkPeApplication(readFileSync(resolve(app)))}:{})},null,2));
}
