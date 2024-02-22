// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod ast;
pub mod comments;
pub(crate) mod filter;
pub mod keywords;
pub mod lexer;
pub(crate) mod syntax;
pub(crate) mod verification_attribute_filter;

use crate::{
    diagnostics::FilesSourceText,
    parser::{self, ast::PackageDefinition, syntax::parse_file_string},
    shared::{CompilationEnv, IndexedPackagePath, NamedAddressMaps},
};
use anyhow::anyhow;
use comments::*;
use move_command_line_common::files::{find_move_filenames, FileHash};
use move_symbol_pool::Symbol;
use std::collections::{BTreeSet, HashMap};
use vfs::VfsPath;

/// Contains the same data as IndexedPackagePath but also information about which file system this
/// path is located in.
pub struct InterfaceFilePath {
    idx_pkg_path: IndexedPackagePath,
    // Virtual file system where this file is located
    vfs: VfsPath,
}

impl InterfaceFilePath {
    pub fn new(idx_pkg_path: IndexedPackagePath, vfs: VfsPath) -> Self {
        InterfaceFilePath { idx_pkg_path, vfs }
    }
}

/// Parses program's targets and dependencies, both of which are read from different virtual file
/// systems (vfs and deps_out_vfs, respectively).
pub(crate) fn parse_program(
    vfs: VfsPath,
    compilation_env: &mut CompilationEnv,
    named_address_maps: NamedAddressMaps,
    targets: Vec<IndexedPackagePath>,
    deps: Vec<IndexedPackagePath>,
    interface_files: Vec<InterfaceFilePath>,
) -> anyhow::Result<(FilesSourceText, parser::ast::Program, CommentMap)> {
    fn find_move_filenames_with_address_mapping(
        paths_with_mapping: Vec<IndexedPackagePath>,
    ) -> anyhow::Result<Vec<IndexedPackagePath>> {
        let mut res = vec![];
        for IndexedPackagePath {
            package,
            path,
            named_address_map: named_address_mapping,
        } in paths_with_mapping
        {
            res.extend(
                find_move_filenames(&[path.as_str()], true)?
                    .into_iter()
                    .map(|s| IndexedPackagePath {
                        package,
                        path: Symbol::from(s),
                        named_address_map: named_address_mapping,
                    }),
            );
        }
        // sort the filenames so errors about redefinitions, or other inter-file conflicts, are
        // deterministic
        res.sort_by(|p1, p2| p1.path.cmp(&p2.path));
        Ok(res)
    }

    let targets = find_move_filenames_with_address_mapping(targets)?;
    let mut deps = find_move_filenames_with_address_mapping(deps)?;
    ensure_targets_deps_dont_intersect(compilation_env, &targets, &mut deps)?;
    let mut files: FilesSourceText = HashMap::new();
    let mut source_definitions = Vec::new();
    let mut source_comments = CommentMap::new();
    let mut lib_definitions = Vec::new();

    for IndexedPackagePath {
        package,
        path,
        named_address_map,
    } in targets
    {
        let (defs, comments, file_hash) =
            parse_file(&vfs, compilation_env, &mut files, path, package)?;
        source_definitions.extend(defs.into_iter().map(|def| PackageDefinition {
            package,
            named_address_map,
            def,
        }));
        source_comments.insert(file_hash, comments);
    }

    for IndexedPackagePath {
        package,
        path,
        named_address_map,
    } in deps
    {
        let (defs, _, _) = parse_file(&vfs, compilation_env, &mut files, path, package)?;
        lib_definitions.extend(defs.into_iter().map(|def| PackageDefinition {
            package,
            named_address_map,
            def,
        }));
    }

    for InterfaceFilePath {
        idx_pkg_path:
            IndexedPackagePath {
                package,
                path,
                named_address_map,
            },
        vfs,
    } in interface_files
    {
        let (defs, _, _) = parse_file(&vfs, compilation_env, &mut files, path, package)?;
        lib_definitions.extend(defs.into_iter().map(|def| PackageDefinition {
            package,
            named_address_map,
            def,
        }));
    }

    let pprog = parser::ast::Program {
        named_address_maps,
        source_definitions,
        lib_definitions,
    };
    Ok((files, pprog, source_comments))
}

fn ensure_targets_deps_dont_intersect(
    compilation_env: &CompilationEnv,
    targets: &[IndexedPackagePath],
    deps: &mut Vec<IndexedPackagePath>,
) -> anyhow::Result<()> {
    // FYI - paths are already canonicalized in Compiler::run
    let target_set = targets.iter().map(|p| p.path).collect::<BTreeSet<_>>();
    let dep_set = deps.iter().map(|p| p.path).collect::<BTreeSet<_>>();
    let intersection = target_set.intersection(&dep_set).collect::<Vec<_>>();
    if intersection.is_empty() {
        return Ok(());
    }
    if compilation_env.flags().sources_shadow_deps() {
        deps.retain(|p| !intersection.contains(&&p.path));
        return Ok(());
    }
    let all_files = intersection
        .into_iter()
        .map(|s| format!("    {}", s))
        .collect::<Vec<_>>()
        .join("\n");
    Err(anyhow!(
        "The following files were marked as both targets and dependencies:\n{}",
        all_files
    ))
}

fn parse_file(
    vfs: &VfsPath,
    compilation_env: &mut CompilationEnv,
    files: &mut FilesSourceText,
    fname: Symbol,
    package: Option<Symbol>,
) -> anyhow::Result<(
    Vec<parser::ast::Definition>,
    MatchedFileCommentMap,
    FileHash,
)> {
    let mut f = vfs.join(fname.as_str())?.open_file()?;
    let mut source_buffer = String::new();
    f.read_to_string(&mut source_buffer)?;
    let file_hash = FileHash::new(&source_buffer);
    let buffer = match verify_string(file_hash, &source_buffer) {
        Err(ds) => {
            compilation_env.add_diags(ds);
            files.insert(file_hash, (fname, source_buffer));
            return Ok((vec![], MatchedFileCommentMap::new(), file_hash));
        }
        Ok(()) => &source_buffer,
    };
    let (defs, comments) = match parse_file_string(compilation_env, file_hash, buffer, package) {
        Ok(defs_and_comments) => defs_and_comments,
        Err(ds) => {
            compilation_env.add_diags(ds);
            (vec![], MatchedFileCommentMap::new())
        }
    };
    files.insert(file_hash, (fname, source_buffer));
    Ok((defs, comments, file_hash))
}
