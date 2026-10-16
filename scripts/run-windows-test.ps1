# Cargo test runner: provide Common Controls v6 only for an unmanifested test
# executable. Existing manifests and all test arguments/results are preserved.
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string] $Executable,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]] $TestArguments
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
if ($env:OS -ne 'Windows_NT') { throw 'This test runner requires Windows.' }
if ($PSVersionTable.PSVersion -lt [version]'7.3') {
    throw 'PowerShell 7.3 or later is required to preserve native argument arrays.'
}
$PSNativeCommandArgumentPassing = 'Standard'
$exe = (Resolve-Path -LiteralPath $Executable).ProviderPath
if ([IO.Path]::GetExtension($exe) -ne '.exe') { throw 'Expected a Cargo test EXE.' }

# LOAD_LIBRARY_AS_DATAFILE | LOAD_LIBRARY_AS_IMAGE_RESOURCE maps resources only:
# this can inspect a loader-failing EXE without resolving imports or running it.
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
namespace ZnetSinkTestManifest {
    public static class Reader {
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, ExactSpelling = true, SetLastError = true)]
        private static extern IntPtr LoadLibraryExW(string file, IntPtr reserved, uint flags);
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, ExactSpelling = true, SetLastError = true)]
        private static extern IntPtr FindResourceW(IntPtr module, IntPtr name, IntPtr type);
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern uint SizeofResource(IntPtr module, IntPtr resource);
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern IntPtr LoadResource(IntPtr module, IntPtr resource);
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern IntPtr LockResource(IntPtr resource);
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool FreeLibrary(IntPtr module);
        public static byte[] Read(string file) {
            IntPtr module = LoadLibraryExW(file, IntPtr.Zero, 0x22);
            if (module == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
            try {
                IntPtr resource = FindResourceW(module, new IntPtr(1), new IntPtr(24));
                if (resource == IntPtr.Zero) {
                    int error = Marshal.GetLastWin32Error();
                    if (error == 1812 || error == 1813 || error == 1814) return null;
                    throw new Win32Exception(error);
                }
                uint size = SizeofResource(module, resource);
                if (size == 0 || size > 1048576) throw new InvalidDataException("Invalid manifest resource size.");
                IntPtr loaded = LoadResource(module, resource);
                if (loaded == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
                IntPtr pointer = LockResource(loaded);
                if (pointer == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
                byte[] bytes = new byte[(int)size];
                Marshal.Copy(pointer, bytes, 0, bytes.Length);
                return bytes;
            } finally { FreeLibrary(module); }
        }
    }
}
'@

$manifest = [ZnetSinkTestManifest.Reader]::Read($exe)
if ($null -eq $manifest) {
    if ([IO.Path]::GetFileName([IO.Path]::GetDirectoryName($exe)) -ine 'deps' -or
        [IO.Path]::GetFileName($exe) -notmatch '.+-[0-9a-f]{16}\.exe$') {
        throw 'Refusing to add a manifest outside a Cargo hashed test EXE in deps/.'
    }
    $mt = Get-Command -Name mt.exe -CommandType Application -ErrorAction SilentlyContinue |
        Select-Object -First 1 -ExpandProperty Source
    if (-not $mt) {
        $kits = Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots' -ErrorAction SilentlyContinue
        if (-not $kits.KitsRoot10) {
            $kits = Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows Kits\Installed Roots' -ErrorAction SilentlyContinue
        }
        if ($kits.KitsRoot10) {
            $versions = Get-ChildItem -LiteralPath (Join-Path $kits.KitsRoot10 'bin') -Directory |
                Where-Object { $_.Name -match '^\d+\.\d+\.\d+\.\d+$' } |
                Sort-Object { [version]$_.Name } -Descending
            foreach ($version in $versions) {
                $candidate = Join-Path $version.FullName 'x64\mt.exe'
                if (Test-Path -LiteralPath $candidate -PathType Leaf) { $mt = $candidate; break }
            }
        }
    }
    if (-not $mt) { throw 'Windows SDK mt.exe was not found; the test EXE was not changed.' }
    $source = Join-Path $PSScriptRoot 'windows-test.manifest.xml'
    & $mt '-nologo' '-manifest' $source "-outputresource:$exe;#1"
    if ($LASTEXITCODE -ne 0) { throw "mt.exe failed to embed the test manifest ($LASTEXITCODE)." }
    if ($null -eq [ZnetSinkTestManifest.Reader]::Read($exe)) {
        throw 'The test manifest was not present after mt.exe completed.'
    }
    Write-Host "Embedded Common Controls v6 manifest into unmanifested test EXE: $exe"
} else {
    Write-Host "Preserved existing RT_MANIFEST #1: $exe"
}

& $exe @TestArguments
exit $LASTEXITCODE
