# SPDX-License-Identifier: GPL-3.0-only
# Copyright (c) 2026 Paddock contributors
[CmdletBinding(SupportsShouldProcess=$true)]
param(
  [string]$SimHubRoot='C:\Program Files (x86)\SimHub',
  [string]$SettingsPath,
  [string]$DashboardPath=(Join-Path $PSScriptRoot 'Paddock CM2.mzdash'),
  [string]$PreviewPath
)
$ErrorActionPreference='Stop'
if(-not $SettingsPath){$SettingsPath=Join-Path $SimHubRoot 'PluginsData\Common\MozaPlugin.MozaPluginSettings.json'}
if(-not $WhatIfPreference -and -not $PreviewPath -and (Get-Process -Name SimHubWPF -ErrorAction SilentlyContinue)){
  throw 'Exit SimHub completely from its tray menu before applying these settings. No files were changed.'
}
Add-Type -Path (Join-Path $SimHubRoot 'Newtonsoft.Json.dll')
$reader=[Newtonsoft.Json.JsonTextReader]::new([IO.StringReader]::new([IO.File]::ReadAllText($SettingsPath)))
$reader.DateParseHandling=[Newtonsoft.Json.DateParseHandling]::None
$config=[Newtonsoft.Json.Linq.JObject]::Load($reader);$reader.Close()
$mappings=[Newtonsoft.Json.Linq.JObject]::Parse([IO.File]::ReadAllText((Join-Path $PSScriptRoot 'Paddock CM2.history-mappings.json')))
$dashes=@($config['DeviceProfiles']['Dashes'].Properties())
if($dashes.Count -ne 1){throw 'Expected one saved CM2 dash. Select the CM2 profile manually if multiple/zero dashboards are configured.'}
$hash=(Get-FileHash -LiteralPath $DashboardPath -Algorithm SHA1).Hash.Substring(0,8).ToLowerInvariant()
$keys=@('wheel:Paddock CM2','builtin:Paddock CM2',('file:'+[IO.Path]::GetFileName($DashboardPath)+':'+$hash))
$profiles=@($dashes[0].Value['Profiles'])
if($profiles.Count -eq 0){throw 'No CM2 profiles found.'}
foreach($profile in $profiles){
  if($null -eq $profile['ChannelMappings']){$profile['ChannelMappings']=[Newtonsoft.Json.Linq.JObject]::new()}
  foreach($key in $keys){
    if($null -eq $profile['ChannelMappings'][$key]){$profile['ChannelMappings'][$key]=[Newtonsoft.Json.Linq.JObject]::new()}
    foreach($entry in $mappings.Properties()){$profile['ChannelMappings'][$key][$entry.Name]=$entry.Value.DeepClone()}
  }
}
$text=$config.ToString([Newtonsoft.Json.Formatting]::Indented)
if($PreviewPath){[IO.File]::WriteAllText($PreviewPath,$text);Write-Output 'Wrote preview configuration only.';return}
if($PSCmdlet.ShouldProcess($SettingsPath,'Back up settings and add Paddock CM2 lap-history and game-running mappings only')){
  $backup=$SettingsPath+'.before-paddock-history-'+(Get-Date -Format 'yyyyMMdd-HHmmss-ffff')+'.bak'
  $temporary=$SettingsPath+'.paddock-tmp'
  [IO.File]::WriteAllText($temporary,$text)
  [IO.File]::Replace($temporary,$SettingsPath,$backup)
  Write-Output ('Applied Paddock mappings. Backup: '+$backup)
  Write-Output 'Restart SimHub, then upload/select Paddock CM2 on the CM2 device.'
}
