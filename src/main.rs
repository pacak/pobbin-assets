use bpaf::Bpaf;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Bpaf)]
#[bpaf(options)]
struct Args {
    #[bpaf(external, optional)]
    fs: Option<Fs>,

    #[bpaf(external, optional)]
    cache: Option<Cache>,

    #[bpaf(external)]
    action: Action,
}

#[derive(Debug, Clone, Bpaf)]
enum Fs {
    Patch {
        /// Patch version of the bundle for the PoE patch CDN.
        #[bpaf(argument("PATCH"))]
        patch: String,
    },
    Web {
        /// Base URL for the bundle.
        #[bpaf(argument("URL"))]
        web: String,
    },
    Local {
        /// Local path to bundle.
        #[bpaf(argument("PATH"))]
        path: String,
    },
}

#[derive(Debug, Clone, Bpaf)]
enum Cache {
    /// In memory filesystem cache.
    InMemoryCache,
    LocalCache {
        /// Local filesystem cache.
        #[bpaf(argument("PATH"))]
        local_cache: std::path::PathBuf,
    },
}

#[derive(Debug, Clone, Bpaf)]
enum Action {
    /// Print the SHA-256 hash of a bundled file.
    #[bpaf(command)]
    Sha(String),
    /// Extract a file to the current directory.
    #[bpaf(command)]
    Extract(String),
    /// Runs the asset pipeline.
    #[bpaf(command)]
    Assets {
        /// Output directory.
        #[bpaf(short('o'), argument("PATH"), fallback("./out".into()))]
        out: std::path::PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let args = args().run();

    tracing_subscriber::fmt::init();

    let fs: Box<dyn pobbin_assets::BundleFs> = match args.fs {
        Some(Fs::Patch { patch }) => Box::new(pobbin_assets::WebBundleFs::cdn(&patch)),
        Some(Fs::Web { web }) => Box::new(pobbin_assets::WebBundleFs::new(web)),
        Some(Fs::Local { path }) => Box::new(pobbin_assets::LocalBundleFs::new(path)),
        None => Box::new(pobbin_assets::WebBundleFs::cdn(
            &pobbin_assets::latest_patch_version()?,
        )),
    };

    let fs: Box<dyn pobbin_assets::BundleFs> = match args.cache {
        Some(Cache::InMemoryCache) => Box::new(pobbin_assets::CacheBundleFs::new(
            fs,
            pobbin_assets::InMemoryCache::new(),
        )),
        Some(Cache::LocalCache { local_cache }) => Box::new(pobbin_assets::CacheBundleFs::new(
            fs,
            pobbin_assets::LocalCache::new(local_cache),
        )),
        None => fs,
    };

    match args.action {
        Action::Sha(file) => sha(fs, &file),
        Action::Extract(file) => extract(fs, &file),
        Action::Assets { out } => assets(fs, out),
    }
}

fn sha<F: pobbin_assets::BundleFs>(fs: F, file: &str) -> anyhow::Result<()> {
    let bundle = pobbin_assets::Bundle::new(fs);
    let index = bundle.index()?;

    let contents = index
        .read_by_name(file)?
        .ok_or_else(|| anyhow::anyhow!("file {file} can not be found"))?;

    let sha256 = {
        let mut hasher = Sha256::new();
        hasher.update(contents);
        hasher.finalize()
    };
    println!("{sha256:x}");

    Ok(())
}

fn extract<F: pobbin_assets::BundleFs>(fs: F, file: &str) -> anyhow::Result<()> {
    let bundle = pobbin_assets::Bundle::new(fs);
    let index = bundle.index()?;

    let contents = index
        .read_by_name(file)?
        .ok_or_else(|| anyhow::anyhow!("file {file} can not be found"))?;

    let path = std::path::PathBuf::from(file);
    std::fs::write(path.file_name().unwrap(), contents)?;

    Ok(())
}

fn assets<F: pobbin_assets::BundleFs>(fs: F, out: std::path::PathBuf) -> anyhow::Result<()> {
    use pobbin_assets::{File, Image, Kind};

    if !out.is_dir() {
        anyhow::bail!("out path '{}' is not a directory", out.display());
    }

    pobbin_assets::Pipeline::new(fs, out)
        .select(|file: &File| file.id.starts_with("Metadata/Items/Gems"))
        .select(|file: &File| file.id.starts_with("Metadata/Items/Belts"))
        .select(|file: &File| file.id.starts_with("Metadata/Items/Rings"))
        .select(|file: &File| file.id.starts_with("Metadata/Items/Flasks"))
        .select(|file: &File| file.id.starts_with("Metadata/Items/Amulets"))
        .select(|file: &File| file.id.starts_with("Metadata/Items/Armours"))
        .select(|file: &File| file.id.starts_with("Metadata/Items/Weapons"))
        .select(|file: &File| file.id.starts_with("Metadata/Items/Trinkets"))
        .select(|file: &File| file.kind == Kind::Unique)
        .postprocess(
            |file: &File| {
                file.id.starts_with("Metadata/Items/Flasks") || file.id.starts_with("UniqueFlask")
            },
            |image: &mut Image| image.flask(),
        )
        .execute()?;

    Ok(())
}
