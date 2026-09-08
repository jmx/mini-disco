# Vendor Minidisc For Upload Parity

Mini Disco vendors the `minidisc` crate while NetMD upload behavior is being proven against real hardware. The local copy removes an extra pre-bulk poll that is not present in `netmd-js`, prevents a secondary encryptor-thread panic after device rejection, and adds the alternate Sony deck EKB used by Web MiniDisc; once these changes are upstreamed or no longer needed, the dependency should return to crates.io.
