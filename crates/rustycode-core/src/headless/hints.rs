/// Heuristic-based hints for common command-line and language errors.
pub fn get_tool_error_hint(command: &str, output: &str) -> Option<String> {
    let _cmd_lower = command.to_lowercase();
    let out_lower = output.to_lowercase();

    // Command timeout — common in QEMU-emulated environments
    if out_lower.contains("command timed out") || out_lower.contains("timed out after") {
        return Some(
            "HINT: The command timed out. This is common in slow/emulated environments. \
            Try: 1) Break into smaller steps, 2) Install deps separately before building, \
            3) Use simpler build flags, 4) Run build in background with `&` and poll."
                .to_string(),
        );
    }

    // Python / Cython hints
    if out_lower.contains(".pyx") && out_lower.contains("attributeerror") {
        return Some("HINT: The error is in a Cython (.pyx) file. After editing .pyx source files, \
            you MUST rebuild: run \"python setup.py build_ext --inplace\" then \"pip install -e .\" \
            to recompile the extension.".to_string());
    }

    // NumPy deprecation hint
    if (out_lower.contains("has no attribute 'int'")
        || out_lower.contains("has no attribute 'float'")
        || out_lower.contains("has no attribute 'complex'")
        || out_lower.contains("has no attribute 'bool'")
        || out_lower.contains("has no attribute 'str'"))
        && (out_lower.contains("numpy") || out_lower.contains("np."))
    {
        return Some("HINT: This is a NumPy 2.0 deprecation error. You MUST search and fix ALL source files \
            — both .py AND .pyx/.pxd files. Common unfixed locations: spacecurve.py, named.py, __init__.py. \
            Use: grep -rn \"np\\.\\(float\\|int\\|complex\\|bool\\)[^0-9_]\" . \
            Replace np.float → float, np.int → int, np.complex → complex, np.bool → bool. \
            After fixing ALL files, rebuild: \"python setup.py build_ext --inplace && pip install -e .\" \
            Then verify: python -c \"from pyknotid.spacecurves import Knot\"".to_string());
    }

    // Missing build dependencies
    if out_lower.contains("no module named 'setuptools'")
        || out_lower.contains("no module named 'cython'")
        || out_lower.contains("modulenotfounderror: no module named 'setuptools'")
        || out_lower.contains("modulenotfounderror: no module named 'cython'")
        || out_lower.contains("command not found: cython")
        || out_lower.contains("error: command 'cython' failed")
        || out_lower.contains("unable to find 'cython'")
    {
        return Some(
            "HINT: Missing Python build dependency. Install with: \
            pip install setuptools wheel cython \
            Then retry the build command."
                .to_string(),
        );
    }

    // Missing module hints
    if out_lower.contains("modulenotfounderror") || out_lower.contains("no module named") {
        let module_hints = [
            ("cv2", "opencv-python"),
            ("PIL", "Pillow"),
            ("sklearn", "scikit-learn"),
            ("scipy", "scipy"),
            ("yaml", "pyyaml"),
            ("Crypto", "pycryptodome"),
            ("bs4", "beautifulsoup4"),
            ("lxml", "lxml"),
            ("pytest", "pytest"),
            ("flask", "flask"),
            ("django", "django"),
            ("requests", "requests"),
            ("boto3", "boto3"),
            ("grpc", "grpcio"),
            ("fastapi", "fastapi"),
            ("pandas", "pandas"),
            ("numpy", "numpy"),
            ("torch", "torch"),
            ("tensorflow", "tensorflow"),
            ("dotenv", "python-dotenv"),
            ("aiohttp", "aiohttp"),
            ("httpx", "httpx"),
        ];
        for (import_name, pip_name) in &module_hints {
            if out_lower.contains(&format!("no module named '{}'", import_name.to_lowercase()))
                || out_lower.contains(&format!(
                    "no module named \"{}\"",
                    import_name.to_lowercase()
                ))
            {
                return Some(format!(
                    "HINT: Install the missing module: pip install {}",
                    pip_name
                ));
            }
        }
    }

    // Rust cargo errors
    if out_lower.contains("cargo ") && out_lower.contains("error") {
        if out_lower.contains("could not find") && out_lower.contains("in crate") {
            return Some(
                "HINT: Missing Rust dependency. Add it to Cargo.toml: \
                check the exact crate name on crates.io and add it under [dependencies]."
                    .to_string(),
            );
        }
        if out_lower.contains("multiple matching crates") {
            return Some(
                "HINT: Ambiguous crate name. Use the full crate path or \
                check crates.io for the exact package name."
                    .to_string(),
            );
        }
        if out_lower.contains("linker")
            && (out_lower.contains("not found") || out_lower.contains("failed"))
        {
            return Some("HINT: Linker error — usually a missing C library. On Ubuntu/Debian: \
                apt install build-essential pkg-config libssl-dev. On macOS: xcode-select --install.".to_string());
        }
    }

    // npm/Node.js errors
    if out_lower.contains("npm err!") || out_lower.contains("npm error") {
        if out_lower.contains("eacces") || out_lower.contains("permission denied") {
            return Some(
                "HINT: npm permission error. Fix with: \
                npm config set prefix ~/.npm-global && export PATH=~/.npm-global/bin:$PATH. \
                Do NOT use sudo npm install."
                    .to_string(),
            );
        }
        if out_lower.contains("enoent") && out_lower.contains("package.json") {
            return Some(
                "HINT: No package.json found. Run 'npm init -y' first, or cd to the project root."
                    .to_string(),
            );
        }
    }

    // Docker errors
    if out_lower.contains("docker") && out_lower.contains("error") {
        if out_lower.contains("permission denied") {
            return Some(
                "HINT: Docker permission error. Add user to docker group: \
                sudo usermod -aG docker $USER && newgrp docker. Or use 'sudo docker'."
                    .to_string(),
            );
        }
        if out_lower.contains("no such image") || out_lower.contains("not found") {
            return Some(
                "HINT: Docker image not found locally. Pull it first: docker pull <image>"
                    .to_string(),
            );
        }
    }

    // TypeScript errors
    if out_lower.contains("tsc") && out_lower.contains("error") && out_lower.contains("ts") {
        return Some(
            "HINT: TypeScript compilation error. Check for: \
            1) Missing type definitions (npm install -D @types/node), \
            2) Incorrect import paths, \
            3) Strict mode violations. Run 'npx tsc --noEmit' for details."
                .to_string(),
        );
    }

    // Git merge conflict
    if out_lower.contains("merge conflict") || out_lower.contains("concurrent modification") {
        return Some(
            "HINT: Merge conflict detected. Resolve by: \
            1) Open conflicting files (search for <<<<<<< markers), \
            2) Choose the correct code section, \
            3) Remove conflict markers, \
            4) git add the resolved files, \
            5) git commit to complete the merge."
                .to_string(),
        );
    }

    // C compiler errors
    if out_lower.contains("gcc") && out_lower.contains("error") {
        if out_lower.contains("implicit declaration") {
            return Some(
                "HINT: Implicit function declaration — missing #include. \
                Add the appropriate header (e.g., #include <stdlib.h> for malloc, \
                #include <string.h> for strlen)."
                    .to_string(),
            );
        }
        if out_lower.contains("undefined reference") {
            return Some(
                "HINT: Undefined reference — linker can't find a function. \
                Check: 1) Function name spelling, 2) Library linking (-l flag), \
                3) Source file compilation order."
                    .to_string(),
            );
        }
    }

    // Permission errors
    if out_lower.contains("permission denied")
        && !out_lower.contains("docker")
        && !out_lower.contains("npm")
        && (out_lower.contains(".sh") || out_lower.contains("script"))
    {
        return Some("HINT: Script not executable. Run: chmod +x <script>.sh".to_string());
    }

    // Port already in use
    if out_lower.contains("address already in use")
        || out_lower.contains("port") && out_lower.contains("in use")
    {
        return Some("HINT: Port already in use. Find the process: lsof -i :<port> or ss -tlnp | grep <port>. \
            Kill it: kill <PID>. Or use a different port.".to_string());
    }

    // SSH/git server errors
    if out_lower.contains("connection refused")
        && (out_lower.contains("git") || out_lower.contains("ssh"))
    {
        return Some(
            "HINT: SSH/git connection refused. Check: 1) sshd is running (service ssh status), \
            2) The service is listening on the expected port, \
            3) No firewall blocking. Start sshd: service ssh start"
                .to_string(),
        );
    }

    // Patch file created but not applied
    if out_lower.contains("wrote")
        && out_lower.contains(".patch")
        && !command.contains("patch ")
        && !command.contains("git apply")
    {
        return Some(
            "HINT: You created a patch file but didn't apply it. Use: patch -p1 < file.patch \
            or: git apply file.patch"
                .to_string(),
        );
    }

    // Build/setup.py errors
    if out_lower.contains("setup.py") && out_lower.contains("error") {
        if out_lower.contains("no module named 'cython'") || out_lower.contains("cython' failed") {
            return Some(
                "HINT: Cython is required for building. Install: pip install cython \
                Then retry: python setup.py build_ext --inplace"
                    .to_string(),
            );
        }
        if out_lower.contains("numpy")
            && (out_lower.contains("deprecated") || out_lower.contains("attributeerror"))
        {
            return Some("HINT: NumPy compatibility error. Fix .pyx and .pxd files: \
                grep -rn 'np\\.(float\\|int\\|complex\\|bool)[^0-9_]' . and replace with Python builtins. \
                Then rebuild: python setup.py build_ext --inplace && pip install -e .".to_string());
        }
    }

    // ... add more as needed ...
    None
}
