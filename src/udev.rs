const NETMD_RULES_PATH: &str = "webminidisc/extra/70-netmd.rules";

pub fn print_doctor() {
    let mut stdout = std::io::stdout();
    write_doctor(&mut stdout).expect("writing to stdout should succeed");
}

pub fn print_doctor_to_stderr() {
    let mut stderr = std::io::stderr();
    write_doctor(&mut stderr).expect("writing to stderr should succeed");
}

fn write_doctor(writer: &mut impl std::io::Write) -> std::io::Result<()> {
    writeln!(writer, "Linux USB diagnostics")?;
    writeln!(writer)?;
    writeln!(
        writer,
        "Mini Disco needs permission to open the NetMD USB device."
    )?;
    writeln!(
        writer,
        "Install udev rules derived from `{NETMD_RULES_PATH}` as:"
    )?;
    writeln!(writer)?;
    writeln!(writer, "  /etc/udev/rules.d/70-netmd.rules")?;
    writeln!(writer)?;
    writeln!(writer, "Then reload rules and reconnect the device:")?;
    writeln!(writer)?;
    writeln!(writer, "  sudo udevadm control --reload-rules")?;
    writeln!(writer, "  sudo udevadm trigger")?;
    writeln!(writer)?;
    writeln!(
        writer,
        "This first CLI only supports Linux USB NetMD devices."
    )?;
    Ok(())
}
