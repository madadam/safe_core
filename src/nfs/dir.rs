// Copyright 2016 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under (1) the MaidSafe.net
// Commercial License, version 1.0 or later, or (2) The General Public License
// (GPL), version 3, depending on which licence you accepted on initial access
// to the Software (the "Licences").
//
// By contributing code to the SAFE Network Software, or to this project
// generally, you agree to be bound by the terms of the MaidSafe Contributor
// Agreement, version 1.0.
// This, along with the Licenses can be found in the root directory of this
// project at LICENSE, COPYING and CONTRIBUTOR.
//
// Unless required by applicable law or agreed to in writing, the SAFE Network
// Software distributed under the GPL Licence is distributed on an "AS IS"
// BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
// implied.
//
// Please review the Licences for the specific language governing permissions
// and limitations relating to use of the SAFE Network Software.

use nfs::errors::NfsError;
use nfs::file::File;
use nfs::metadata::DirMetadata;
use routing::{DataIdentifier, XorName};
use rust_sodium::crypto::{box_, secretbox};
use std::cmp;

/// Shorthand type for directory identifiers
pub type DirId = (DataIdentifier, Option<secretbox::Key>);

/// Struct that represent a directory in the network.
#[derive(Clone, Debug, Eq, PartialEq, RustcEncodable, RustcDecodable)]
pub struct Dir {
    sub_dirs: Vec<DirMetadata>,
    files: Vec<File>,
}

impl Dir {
    /// Create a new, empty Dir.
    pub fn new() -> Self {
        Dir {
            sub_dirs: Vec::new(),
            files: Vec::new(),
        }
    }

    /// Get all files in this Directory
    pub fn files(&self) -> &[File] {
        &self.files
    }

    /// Get all files in this Directory with mutability to update the listing
    /// of files
    pub fn files_mut(&mut self) -> &mut Vec<File> {
        &mut self.files
    }

    /// Find file in this Directory by name.
    pub fn find_file(&self, file_name: &str) -> Option<&File> {
        self.files().iter().find(|file| *file.name() == *file_name)
    }

    /// Get all subdirectories in this Directory.
    pub fn sub_dirs(&self) -> &[DirMetadata] {
        &self.sub_dirs
    }

    /// Get all subdirectories in this Directory with mutability to update the
    /// listing of subdirectories.
    pub fn sub_dirs_mut(&mut self) -> &mut Vec<DirMetadata> {
        &mut self.sub_dirs
    }

    /// Find sub-directory of this Directory by name.
    pub fn find_sub_dir(&self, directory_name: &str) -> Option<&DirMetadata> {
        self.sub_dirs().iter().find(|info| *info.name() == *directory_name)
    }

    /// Find sub-directory of this Directory by id.
    pub fn find_sub_dir_by_id(&self, id: &DataIdentifier) -> Option<&DirMetadata> {
        self.sub_dirs().iter().find(|info| *info.locator() == *id)
    }

    /// Add a new file to this Dir
    pub fn add_file(&mut self, file: File) -> Result<(), NfsError> {
        if let Some(_) = self.find_file(file.name()) {
            return Err(NfsError::FileAlreadyExistsWithSameName);
        }
        self.files_mut().push(file);
        Ok(())
    }

    /// If file is present in this Dir then replace it
    pub fn upsert_file(&mut self, file: File) -> Result<(), NfsError> {
        if let Some(_) = self.find_file(file.name()) {
            let filename = file.name().to_owned();
            self.update_file(&filename, file);
            Ok(())
        } else {
            self.add_file(file)
        }
    }

    /// Updates file previously known by a specified name
    /// Returns true if file was updated
    pub fn update_file(&mut self, prev_name: &str, file: File) -> bool {
        if let Some(index) = self.files()
            .iter()
            .position(|entry| entry.name() == prev_name) {
            let mut existing = unwrap!(self.files_mut().get_mut(index));
            *existing = file;
            true
        } else {
            false
        }
    }

    /// Remove a file
    pub fn remove_file(&mut self, file_name: &str) -> Result<File, NfsError> {
        let index = self.files()
            .iter()
            .position(|file| *file.name() == *file_name)
            .ok_or(NfsError::FileNotFound)?;
        Ok(self.files_mut().remove(index))
    }

    /// If DirMetadata is present in the sub_dirs of this Directory
    /// then replace it else insert it
    pub fn upsert_sub_dir(&mut self, dir_metadata: DirMetadata) -> Result<(), NfsError> {
        if let Some(index) = self.sub_dirs()
            .iter()
            .position(|entry| *entry.locator() == *dir_metadata.locator()) {
            if self.sub_dirs()[index].name() != dir_metadata.name() &&
               self.find_sub_dir(dir_metadata.name()).is_some() {
                return Err(NfsError::DirectoryAlreadyExistsWithSameName);
            }
            self.sub_dirs_mut()[index] = dir_metadata;
        } else {
            if self.find_sub_dir(dir_metadata.name()).is_some() {
                return Err(NfsError::DirectoryAlreadyExistsWithSameName);
            }
            self.sub_dirs_mut().push(dir_metadata);
        }
        Ok(())
    }

    /// Remove a sub_directory
    pub fn remove_sub_dir(&mut self, directory_name: &str) -> Result<DirMetadata, NfsError> {
        let index = self.sub_dirs()
            .iter()
            .position(|dir_info| *dir_info.name() == *directory_name)
            .ok_or(NfsError::DirectoryNotFound)?;
        Ok(self.sub_dirs_mut().remove(index))

    }

    // Generates a nonce based on the directory_id
    #[allow(missing_docs)]
    pub fn generate_nonce(directory_id: &XorName) -> box_::Nonce {
        let mut nonce = [0u8; box_::NONCEBYTES];
        let min_length = cmp::min(nonce.len(), directory_id.0.len());
        nonce.clone_from_slice(&directory_id.0[..min_length]);
        box_::Nonce(nonce)
    }
}

#[cfg(test)]
mod tests {
    use maidsafe_utilities::serialisation::{deserialise, serialise};
    use nfs::file::File;
    use nfs::metadata::{DirMetadata, FileMetadata};
    use rand;
    use self_encryption::DataMap;
    use super::Dir;

    fn create_directory(name: &str, user_metadata: Vec<u8>) -> DirMetadata {
        let id = rand::random();
        DirMetadata::new(id, name, user_metadata, None)
    }

    #[test]
    fn serialise_and_deserialise_directory() {
        let dir = create_directory("Home",
                                   "some metadata about the directory"
                                       .to_string()
                                       .into_bytes());

        let serialised_content = unwrap!(serialise(&dir));
        let content_after = unwrap!(deserialise(&serialised_content));
        assert_eq!(dir, content_after);
    }

    #[test]
    fn find_add_update_remove_file() {
        let mut dir = Dir::new();
        let file = File::Unversioned(FileMetadata::new("index.html".to_string(),
                                                       Vec::new(),
                                                       DataMap::None));
        assert!(dir.find_file(file.name()).is_none());

        unwrap!(dir.add_file(file.clone()));
        assert!(dir.find_file(file.name()).is_some());

        let mut metadata = file.metadata().clone();
        metadata.set_name("home.html".to_string());

        dir.update_file("index.html", File::Unversioned(metadata));
        assert_eq!(dir.files().len(), 1);
        let file2 = File::Unversioned(FileMetadata::new("demo.html".to_string(),
                                                        Vec::new(),
                                                        DataMap::None));
        unwrap!(dir.add_file(file2.clone()));
        assert_eq!(dir.files().len(), 2);

        let _ = unwrap!(dir.find_file("home.html"), "File not found");
        let _ = unwrap!(dir.find_file(file2.name()), "File not found");

        let _ = unwrap!(dir.remove_file("home.html"));
        assert!(dir.find_file("home.html").is_none());
        assert!(dir.find_file(file2.name()).is_some());
        assert_eq!(dir.files().len(), 1);

        let _ = unwrap!(dir.remove_file(file2.metadata().name()));
        assert_eq!(dir.files().len(), 0);
    }

    #[test]
    fn find_upsert_remove_directory() {
        let mut dir = Dir::new();
        let mut sub_dir = create_directory("Child one", Vec::new());
        assert!(dir.find_sub_dir(sub_dir.name())
            .is_none());
        unwrap!(dir.upsert_sub_dir(sub_dir.clone()));
        assert!(dir.find_sub_dir(sub_dir.name())
            .is_some());

        sub_dir.set_name("Child_1".to_string());
        unwrap!(dir.upsert_sub_dir(sub_dir.clone()));
        assert_eq!(dir.sub_dirs().len(), 1);

        let sub_dir_two = create_directory("Child Two", Vec::new());
        unwrap!(dir.upsert_sub_dir(sub_dir_two.clone()));
        assert_eq!(dir.sub_dirs().len(), 2);

        let _ = unwrap!(dir.find_sub_dir(sub_dir.name()), "Directory not found");
        let _ = unwrap!(dir.find_sub_dir(sub_dir_two.name()), "Directory not found");

        let _ = unwrap!(dir.remove_sub_dir(sub_dir.name()));
        assert!(dir.find_sub_dir(sub_dir.name())
            .is_none());
        assert!(dir.find_sub_dir(sub_dir_two.name())
            .is_some());
        assert_eq!(dir.sub_dirs().len(), 1);

        // TODO (Spandan) - Fetch and issue a DELETE on the removed directory, check
        // elsewhere in code/test. Also check what can be done for file removals.
        let _ = unwrap!(dir.remove_sub_dir(sub_dir_two.name()));
        assert_eq!(dir.sub_dirs().len(), 0);
    }
}
