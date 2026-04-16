// Copyright (C) 2026 M.R. Siavash Katebzadeh <mr@katebzadeh.xyz>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Serialize, de::DeserializeOwned};

use crate::{CoreResult, Error};

pub(crate) fn load_json_or_default<T, FParse, FRead>(
    path: &Path,
    parse_err: FParse,
    read_err: FRead,
) -> CoreResult<T>
where
    T: DeserializeOwned + Default,
    FParse: Fn(PathBuf, serde_json::Error) -> Error,
    FRead: Fn(PathBuf, io::Error) -> Error,
{
    match fs::read_to_string(path) {
        Ok(contents) => {
            let store = serde_json::from_str(&contents)
                .map_err(|source| parse_err(path.to_path_buf(), source))?;
            Ok(store)
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(T::default()),
        Err(source) => Err(read_err(path.to_path_buf(), source).into()),
    }
}

pub(crate) fn save_json_pretty<T, FWrite>(
    path: &Path,
    value: &T,
    write_err: FWrite,
) -> CoreResult<()>
where
    T: Serialize,
    FWrite: Fn(PathBuf, io::Error) -> Error,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| write_err(path.to_path_buf(), source))?;
    }

    let tmp_path = path.with_extension("json.tmp");
    let payload = serde_json::to_string_pretty(value)
        .map_err(|source| write_err(path.to_path_buf(), io::Error::other(source)))?;

    fs::write(&tmp_path, payload).map_err(|source| write_err(path.to_path_buf(), source))?;
    fs::rename(&tmp_path, path).map_err(|source| write_err(path.to_path_buf(), source))?;

    Ok(())
}

pub(crate) fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}
