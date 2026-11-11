fn main() {
    let args: Vec<_> = std::env::args().collect();
    let result = || -> yeskia_core::Result<()> {
        if args.len() != 3 {
            return Err(yeskia_core::Error::InvalidArgument(
                "usage: tls-verify <mbedtls-lib|ssl-patch> <local-file>".into(),
            ));
        }
        let bytes = yeskia_core::revive::read_bounded(std::path::Path::new(&args[2]))?;
        let result = yeskia_core::revive::validate(&args[1], &bytes)?;
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
        Ok(())
    };
    if let Err(e) = result() {
        eprintln!("{}", e.to_info().message);
        std::process::exit(1);
    }
}
