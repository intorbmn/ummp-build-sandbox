$OUT = $PSScriptRoot
$STAGE = Join-Path $OUT "_stage"
Remove-Item -Recurse -Force $STAGE -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $STAGE | Out-Null

function Write-Jar($name, $entries) {
    $dir = Join-Path $STAGE $name
    Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    foreach ($k in $entries.Keys) {
        $full = Join-Path $dir $k
        $parent = Split-Path $full -Parent
        if (-not (Test-Path $parent)) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
        [System.IO.File]::WriteAllText($full, $entries[$k])
    }
    $zip = Join-Path $OUT ($name + ".zip")
    if ($entries.Count -eq 0) {
        # .NET zip requires at least one entry; write a sentinel, strip it after.
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $za = [System.IO.Compression.ZipFile]::Open($zip, 'Create')
        $e = $za.CreateEntry("")
        $w = $e.Open(); $w.Close()
        $za.Dispose()
    } else {
        Get-ChildItem -Path $dir -Recurse | Compress-Archive -DestinationPath $zip -Force
    }
    Move-Item -Path $zip -Destination (Join-Path $OUT $name) -Force
    Write-Host "wrote $name"
}

$manifestMTF = "Implementation-Title: ManifestMod`nImplementation-Version: 3.1.4`nMod-ID: manifestmod`nBuild-Jdk: 17.0.12`n"

Write-Jar "fabric-a.jar" @{ "fabric.mod.json" = '{"id":"fabrica","version":"1.2.0","name":"Fabrica","depends":{"minecraft":"1.20.1","fabricloader":">=0.14.0","fabric-api":">=0.80.0"}}'; "META-INF/MANIFEST.MF" = "Implementation-Title: Fabrica`nImplementation-Version: 1.2.0`n" }
Write-Jar "fabric-b.jar" @{ "fabric.mod.json" = '{"id":"fabricb","version":"0.9.3","name":"FabricB","depends":{"minecraft":"1.21","fabrica":">=1.0.0"}}' }
Write-Jar "fabric-c.jar" @{ "fabric.mod.json" = '{"id":"fabricc","version":"2.0.0","name":"FabricC","depends":{"minecraft":"1.20.1","fabricb":">=0.9.0","fabrica":">=1.0.0"}}' }

Write-Jar "forge-a.jar" @{ "META-INF/mods.toml" = "modId = `"forgea`"`nversion = `"1.4.0`"`ndisplayName = `"ForgeA`"`n[[dependencies]]`nmodId = `"minecraft`"`nversionRange = `"1.20.1`"`nmandatory = true`n[[dependencies]]`nmodId = `"fabricb`"`nversionRange = `">=0.9.0`"`nmandatory = false`n"; "META-INF/MANIFEST.MF" = "Implementation-Title: ForgeA`n" }
Write-Jar "neoforge-a.jar" @{ "META-INF/mods.toml" = "modId = `"neoforgea`"`nversion = `"21.0.50`"`ndisplayName = `"NeoForgeA`"`nloader = `"`NEOFORGE`"`nminecraft = `"1.20.1`"`n" }
Write-Jar "forge-b.jar" @{ "META-INF/mods.toml" = "modId = `"forgeb`"`nversion = `"0.7.1`"`ndisplayName = `"ForgeB`"`nminecraft = `"1.21`"`n" }

Write-Jar "legacy-a.jar" @{ "mcmod.info" = '[{"modid":"legacya","version":"0.5.0","name":"LegacyA","mcversion":"1.12.2"}]' }
Write-Jar "manifest-a.jar" @{ "META-INF/MANIFEST.MF" = $manifestMTF }
Write-Jar "empty.jar" @{}

$kotlinDir = Join-Path $OUT "kotlin-src"
Remove-Item -Recurse -Force $kotlinDir -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $kotlinDir | Out-Null
Set-Content -Path (Join-Path $kotlinDir "Example.kt") -Value "fun main() { println(`"kotlin`") }`n" -NoNewline

Remove-Item -Recurse -Force $STAGE -ErrorAction SilentlyContinue
Write-Host "done"