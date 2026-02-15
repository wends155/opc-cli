# opc-cli

A high-performance Terminal User Interface (TUI) for OPC DA interaction, built with Rust.

## 🚀 Quick Start

### Prerequisites
- Windows OS (Required for OPC DA/COM interaction)
- Rust (2024 Edition)
- PowerShell or GNU Make

### Building

**Using Make:**
```powershell
make debug    # Fast build
make release  # Optimized build
```

**Using PowerShell:**
```powershell
./scripts/package.ps1 debug
./scripts/package.ps1 release
```

### Testing
```powershell
make test
# OR
./scripts/package.ps1 test
```

### Packaging for Deployment
To create a deployment bundle (EXE + PDB + metadata):
```powershell
make package
# OR
./scripts/package.ps1 package
```
This generates `opc-cli-dist.zip` and the `dist/` folder.

## 🏗️ Architecture
See [architecture.md](architecture.md) for a deep dive into the design patterns and components.

## 📝 Documentation
- [Walkthrough](.gemini/antigravity/brain/0c437b09-0259-476f-b95b-1ac7669aaa3b/walkthrough.md): Achievement summary and validation proof.
- [Context](context.md): Project history and key decisions.
