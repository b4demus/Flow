# Installing Facet

Windows x64 · After Effects · 41 AEX plugins

1. Save your project and close After Effects, aerender and other Adobe apps
   that may be using the plugins.
2. Run `Facet-Setup-2026-10-21.exe` and click **Установить / Install**.
   The current setup interface is in Russian. Alternatively, extract the ZIP
   completely and run `install.cmd`.
3. Accept the standard Windows administrator prompt.
4. Once installation succeeds, start AE. Look for the **Facet ...** effect categories.

The EXE contains the complete package: no adjacent ZIP, internet connection or
Rust installation is required. Setup uses Windows PowerShell 5.1 and .NET
Framework 4.8. The executable is not publisher-signed.

Plugins are installed to the shared Adobe directory:

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\Facet
```

Updates affect Adobe applications that use this shared directory. The CLI stays
in the ZIP's `dist` folder; setup does not add it to PATH.

Setup verifies the binary hashes, plugin identities and license files. It
replaces older Facet builds, recognizes legacy filenames, and consolidates
duplicates from MediaCore and the standard plugin directories of installed
After Effects versions. Unrelated plugins are left in place. Custom plugin
locations outside these paths are not scanned automatically.

Adobe applications are never terminated automatically. Setup refuses to write
while a supported Adobe host is running, and prevents concurrent installations.
It does not change system ExecutionPolicy or broaden plugin-folder permissions.

## Backups and restore

Before changing files, setup saves their originals and a journal to:

```text
C:\ProgramData\Facet\Backups\<timestamp-identifier>
```

The exact directory is shown after installation. Write failures trigger an
automatic rollback. To restore an earlier installation manually, extract the ZIP,
close AE, and run:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\install.ps1 -Restore 'C:\ProgramData\Facet\Backups\<timestamp-identifier>'
```

Restore verifies the backup and refuses to overwrite files edited after
installation. It restores old filenames and locations as well, so an old
duplicate-plugin problem may also return.

Read-only package validation and installation plan:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\install.ps1 -Check
```

Verify an EXE's embedded package without installing:

```text
Facet-Setup-2026-10-21.exe /verify <absolute-path-to-result.json>
```

These checks require neither administrator elevation nor closing AE.
See [build status](docs/status.md) for effect limitations.

This software is based in part on the work of the Independent JPEG Group.
The license and notices are installed alongside the plugins.
