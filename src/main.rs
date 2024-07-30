/* SPDX-License-Identifier: GPL-3.0-or-later */

mod handlers;
mod multiprocess;
mod options;
mod simplelog;

use anyhow::{anyhow, bail, Result};
use log::debug;
use std::env;
use std::path::{Path, PathBuf};
use std::rc::Rc;

fn brp_check(config: &options::Config) -> Result<()> {
    // env::current_exe() does readlink("/proc/self/exe"), which returns
    // the target binary, so we cannot use that.

    let arg0 = env::args().next().unwrap();

    debug!("Running as {arg0}… (brp={})", if config.brp { "true" } else { "false" });

    if config.brp {
        let build_root = env::var("RPM_BUILD_ROOT")
            .map_err(|e| anyhow!("$RPM_BUILD_ROOT is not set correctly: {e}"))?;

        if build_root.is_empty() {
            bail!("Empty $RPM_BUILD_ROOT is not allowed");
        }

        // Canonicalize the path, removing duplicate or trailing slashes
        // and intermediate dot components, but not double dots.
        let build_root = PathBuf::from_iter(Path::new(&build_root).iter());

        if build_root == Path::new("/") {
            bail!("RPM_BUILD_ROOT={build_root:?} is not allowed");
        }

        for arg in &config.inputs {
            if !arg.starts_with(&build_root) {
                bail!("Path {arg:?} does not start with RPM_BUILD_ROOT={build_root:?}");
            }
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    let config = match options::Config::make()? {
        None => { return Ok(()); }
        Some(some) => some
    };
    let config = Rc::new(config);

    brp_check(&config)?;

    let stats;

    if let Some(socket) = config.job_socket {
        debug!("Running as worker on job socket {}", socket);
        return multiprocess::do_worker_work(&config);

    } else if let Some(jobs) = config.jobs {
        debug!("Running as controller with {} workers", jobs);
        stats = multiprocess::Controller::do_work(&config)?;

    } else {
        // We're not the controller
        stats = handlers::do_normal_work(&config)?;
    }

    stats.summarize();

    if (config.check || !config.brp) && stats.errors > 0 {
        // Make any errors fatal, except when --brp (without --check) is used.
        // In normal rpms builds, we don't want the build to fail.
        bail!("processing failed")
    } else if config.check && stats.misunderstood > 0 {
        bail!("--check was specified, but some files couldn't be processed")
    } else if config.check && (stats.inodes_replaced > 0 ||
                               stats.inodes_rewritten > 0) {
        bail!("--check was specified, but some files would have been modified")
    }  else {
        Ok(())
    }
}
