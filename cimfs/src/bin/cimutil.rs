use tracing_subscriber::{EnvFilter, util::SubscriberInitExt};
use tracing::trace;
use windows::core::Result;

fn main() -> Result<()> {
    let sub = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_env_filter(
            EnvFilter::builder()
                .from_env()
                .expect("should be able to build from env variables")
                .add_directive(
                    "trace"
                        .parse()
                        .expect("should be able to parse tracing settings"),
                )
                .add_directive(
                    "cimfs=trace"
                        .parse()
                        .expect("should be able to parse tracing settings"),
                ),
        )
        .compact()
        .finish();

    sub.try_init().expect("should init");
    
    cimfs::util::setup_privileges()?;

    trace!("Starting cimtuil");
    let mut image = cimfs::Image::new("C:\\cim", "test0.cim");
    image.create(None)?;

    image.create_file("ntdll.dll", "C:\\Windows\\System32\\ntdll.dll")?;

    image.commit()?;

    Ok(())
}
