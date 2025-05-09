use crate::bindings::*;
use alloc::{ffi::CString, vec::Vec};
use core::{slice, mem, convert::TryInto};
use byteorder::{LittleEndian, ByteOrder};

// Ext4File文件操作与block device设备解耦了
pub struct Ext4File {
    //file_desc_map: BTreeMap<CString, ext4_file>,
    file_desc: ext4_file,
    file_path: CString,
    this_type: InodeTypes,
}

pub struct InodeInfo {
    pub dev : u64,
    pub st_ino: u32,
    pub nlink: u32,
    pub uid: u16,
    pub gid: u16,
    pub nblk_lo: u32,
    pub atime: u32,
    pub mtime: u32,
    pub ctime: u32,
    pub atime_ex: u32,
    pub mtime_ex: u32,
    pub ctime_ex: u32,
}

impl InodeInfo {
    pub fn dev(&self) -> u64 {self.dev}
    pub fn st_ino(&self) -> u32 {self.st_ino}
    pub fn nlink(&self) -> u32 {self.nlink}
    pub fn uid(&self) -> u16 {self.uid}
    pub fn gid(&self) -> u16 {self.gid}
    pub fn nblk_lo(&self) -> u32 {self.nblk_lo}

    pub fn atime(&self) -> u32 {self.atime}
    pub fn mtime(&self) -> u32 {self.mtime}
    pub fn ctime(&self) -> u32 {self.ctime}

    pub fn atime_ex(&self) -> u32 {self.atime_ex}
    pub fn mtime_ex(&self) -> u32 {self.mtime_ex}
    pub fn ctime_ex(&self) -> u32 {self.ctime_ex}
}

pub fn to_inode_info(ino: u32, inode: &ext4_inode) -> InodeInfo {
    InodeInfo {
        // dev: u64::from(LittleEndian::read_u16(&inode.blocks[0].to_ne_bytes())),
        dev: 0,
        st_ino: ino,
        nlink: u32::from(LittleEndian::read_u16(&inode.links_count.to_ne_bytes())),
        uid: u16::from(LittleEndian::read_u16(&inode.uid.to_ne_bytes())),
        gid: u16::from(LittleEndian::read_u16(&inode.gid.to_ne_bytes())),
        nblk_lo: u32::from(LittleEndian::read_u32(&inode.blocks_count_lo.to_ne_bytes())),
        atime: u32::from(LittleEndian::read_u32(&inode.access_time.to_ne_bytes())),
        ctime: u32::from(LittleEndian::read_u32(&inode.change_inode_time.to_ne_bytes())),
        mtime: u32::from(LittleEndian::read_u32(&inode.modification_time.to_ne_bytes())),
        atime_ex: u32::from(LittleEndian::read_u32(&inode.atime_extra.to_ne_bytes())),
        ctime_ex: u32::from(LittleEndian::read_u32(&inode.ctime_extra.to_ne_bytes())),
        mtime_ex: u32::from(LittleEndian::read_u32(&inode.mtime_extra.to_ne_bytes())),
    }
}

impl Ext4File {
    pub fn new(path: &str, types: InodeTypes) -> Self {
        Self {
            file_desc: ext4_file {
                mp: core::ptr::null_mut(),
                inode: 0,
                flags: 0,
                fsize: 0,
                fpos: 0,
            },
            file_path: CString::new(path).expect("CString::new Ext4File path failed"),
            this_type: types,
        }
    }

    pub fn get_path(&self) -> CString {
        self.file_path.clone()
    }

    pub fn get_type(&self) -> InodeTypes {
        self.this_type.clone()
    }

    pub fn get_inode(&self) -> Result<InodeInfo,i32> {
        let mut rt_ino: u32 = 0;
        let bytes: [u8; mem::size_of::<ext4_inode>()] = [0; mem::size_of::<ext4_inode>()];

        let mut inode = unsafe{
            if bytes.len() < mem::size_of::<ext4_inode>() {
                core::panic!("Input bytes too short for ext4_inode");
            }
            let ptr = bytes.as_ptr() as *mut ext4_inode;
            ptr.read_unaligned()
        };

        //TODO:check inode safety
        let ret= unsafe {ext4_raw_inode_fill(self.get_path().into_raw() ,&mut rt_ino as *mut u32, &mut inode as *mut ext4_inode)};

        let result = if ret == 0 {
            Ok(to_inode_info(rt_ino, &inode))
        }else {
            Err(-1)
        };
        result
    }

    /// File open function.
    ///
    /// |---------------------------------------------------------------|
    /// |   r or rb                 O_RDONLY                            |
    /// |---------------------------------------------------------------|
    /// |   w or wb                 O_WRONLY|O_CREAT|O_TRUNC            |
    /// |---------------------------------------------------------------|
    /// |   a or ab                 O_WRONLY|O_CREAT|O_APPEND           |
    /// |---------------------------------------------------------------|
    /// |   r+ or rb+ or r+b        O_RDWR                              |
    /// |---------------------------------------------------------------|
    /// |   w+ or wb+ or w+b        O_RDWR|O_CREAT|O_TRUNC              |
    /// |---------------------------------------------------------------|
    /// |   a+ or ab+ or a+b        O_RDWR|O_CREAT|O_APPEND             |
    /// |---------------------------------------------------------------|
    pub fn file_open(&mut self, path: &str, flags: u32) -> Result<usize, i32> {
        let c_path = CString::new(path).expect("CString::new failed");
        if c_path != self.get_path() {
            trace!(
                "Ext4File file_open, cur path={}, new path={}",
                self.file_path.to_str().unwrap(),
                path
            );
        }
        //let to_map = c_path.clone();
        let c_path = c_path.into_raw();
        let flags = Self::flags_to_cstring(flags);
        let flags = flags.into_raw();

        let r = unsafe { ext4_fopen(&mut self.file_desc, c_path, flags) };
        unsafe {
            // deallocate the CString
            drop(CString::from_raw(c_path));
            drop(CString::from_raw(flags));
        }
        if r != EOK as i32 {
            error!("ext4_fopen: {}, rc = {}", path, r);
            return Err(r);
        }
        //self.file_desc_map.insert(to_map, fd); // store c_path
        trace!("file_open {}, mp={:#x}", path, self.file_desc.mp as usize);
        Ok(EOK as usize)
    }

    pub fn file_close(&mut self) -> Result<usize, i32> {
        if self.file_desc.mp != core::ptr::null_mut() {
            trace!("file_close {:?}", self.get_path());
            // self.file_cache_flush()?;
            unsafe {
                ext4_fclose(&mut self.file_desc);
            }
        }
        Ok(0)
    }

    pub fn flags_to_cstring(flags: u32) -> CString {
        let cstr = match flags {
            O_RDONLY => "rb",
            O_RDWR => "r+",
            0x241 => "wb", // O_WRONLY | O_CREAT | O_TRUNC
            0x441 => "ab", // O_WRONLY | O_CREAT | O_APPEND
            0x242 => "w+", // O_RDWR | O_CREAT | O_TRUNC
            0x442 => "a+", // O_RDWR | O_CREAT | O_APPEND
            _ => {
                warn!("Unknown File Open Flags: {:#x}", flags);
                "r+"
            }
        };
        trace!("flags_to_cstring: {}", cstr);
        CString::new(cstr).expect("CString::new OpenFlags failed")
    }

    /// Inode types:
    /// EXT4_DIRENTRY_UNKNOWN
    /// EXT4_DE_REG_FILE
    /// EXT4_DE_DIR
    /// EXT4_DE_CHRDEV
    /// EXT4_DE_BLKDEV
    /// EXT4_DE_FIFO
    /// EXT4_DE_SOCK
    /// EXT4_DE_SYMLINK
    ///
    /// Check if inode exists.
    pub fn check_inode_exist(&mut self, path: &str, types: InodeTypes) -> bool {
        let c_path = CString::new(path).expect("CString::new failed");
        let c_path = c_path.into_raw();
        let mtype = types.clone();
        let r = unsafe { ext4_inode_exist(c_path, types as i32) }; //eg: types: EXT4_DE_REG_FILE
        unsafe {
            drop(CString::from_raw(c_path));
        }
        if r == EOK as i32 {
            trace!("{:?} {} Exist", mtype, path);
            true //Exist
        } else {
            trace!("{:?} {} No Exist. ext4_inode_exist rc = {}", mtype, path, r);
            false
        }
    }

    /// Rename file and directory
    pub fn file_rename(&mut self, path: &str, new_path: &str) -> Result<usize, i32> {
        let c_path = CString::new(path).expect("CString::new failed");
        let c_path = c_path.into_raw();
        let c_new_path = CString::new(new_path).expect("CString::new failed");
        let c_new_path = c_new_path.into_raw();
        let r = unsafe { ext4_frename(c_path, c_new_path) };
        unsafe {
            drop(CString::from_raw(c_path));
            drop(CString::from_raw(c_new_path));
        }
        if r != EOK as i32 {
            error!("ext4_frename error: rc = {}", r);
            return Err(r);
        }
        Ok(EOK as usize)
    }

    /// Remove file by path.
    pub fn file_remove(&mut self, path: &str) -> Result<usize, i32> {
        trace!("file_remove {}", path);

        let c_path = CString::new(path).expect("CString::new failed");
        let c_path = c_path.into_raw();

        let r = unsafe { ext4_fremove(c_path) };
        unsafe {
            drop(CString::from_raw(c_path));
        }
        if (r != EOK as i32) && (r != ENOENT as i32) {
            error!("ext4_fremove error: rc = {}", r);
            return Err(r);
        }
        Ok(EOK as usize)
    }

    pub fn file_seek(&mut self, offset: i64, seek_type: u32) -> Result<usize, i32> {
        let mut offset = offset;
        let size = self.file_size() as i64;

        if offset > size {
            warn!("Seek beyond the end of the file");
            offset = size;
        }

        let r = unsafe { ext4_fseek(&mut self.file_desc, offset, seek_type) };
        if r != EOK as i32 {
            error!("ext4_fseek: rc = {}", r);
            return Err(r);
        }
        Ok(EOK as usize)
    }

    pub fn file_read(&mut self, buff: &mut [u8]) -> Result<usize, i32> {
        let mut rw_count = 0;
        let r = unsafe {
            ext4_fread(
                &mut self.file_desc,
                buff.as_mut_ptr() as _,
                buff.len(),
                &mut rw_count,
            )
        };

        if r != EOK as i32 {
            error!("ext4_fread: rc = {}", r);
            return Err(r);
        }

        trace!("file_read {:?}, len={}", self.get_path(), rw_count);

        Ok(rw_count)
    }

    /*
    pub fn file_close(&mut self, path: &str) -> Result<usize, i32> {
        let cstr_path = CString::new(path).unwrap();
        if let Some(mut fd) = self.file_desc_map.remove(&cstr_path) {
            unsafe {
                ext4_fclose(&mut fd);
            }
            Ok(0)
        } else {
            error!("Can't find file descriptor of {}", path);
            Err(-1)
        }
    }
    */

    pub fn file_write(&mut self, buf: &[u8]) -> Result<usize, i32> {
        let mut rw_count = 0;
        let r = unsafe {
            ext4_fwrite(
                &mut self.file_desc,
                buf.as_ptr() as _,
                buf.len(),
                &mut rw_count,
            )
        };

        if r != EOK as i32 {
            error!("ext4_fwrite: rc = {}", r);
            return Err(r);
        }
        trace!("file_write {:?}, len={}", self.get_path(), rw_count);
        Ok(rw_count)
    }

    pub fn file_truncate(&mut self, size: u64) -> Result<usize, i32> {
        trace!("file_truncate to {}", size);
        let r = unsafe { ext4_ftruncate(&mut self.file_desc, size) };
        if r != EOK as i32 {
            error!("ext4_ftruncate: rc = {}", r);
            return Err(r);
        }
        Ok(EOK as usize)
    }

    pub fn file_size(&mut self) -> u64 {
        //注，记得先 O_RDONLY 打开文件
        unsafe { ext4_fsize(&mut self.file_desc) }
    }

    pub fn file_cache_flush(&mut self) -> Result<usize, i32> {
        let c_path = self.file_path.clone();
        let c_path = c_path.into_raw();
        unsafe {
            let r = ext4_cache_flush(c_path);
            if r != EOK as i32 {
                error!("ext4_cache_flush: rc = {}", r);
                return Err(r);
            }
            drop(CString::from_raw(c_path));
        }
        Ok(0)
    }

    pub fn file_mode_get(&mut self) -> Result<u32, i32> {
        // 0o777 (octal) == rwxrwxrwx
        let mut mode: u32 = 0o777;
        let c_path = self.file_path.clone();
        let c_path = c_path.into_raw();
        let r = unsafe { ext4_mode_get(c_path, &mut mode) };
        unsafe {
            drop(CString::from_raw(c_path));
        }
        if r != EOK as i32 {
            error!("ext4_mode_get: rc = {}", r);
            return Err(r);
        }
        trace!("Got file mode={:#x}", mode);
        Ok(mode)
    }

    pub fn file_mode_set(&mut self, mode: u32) -> Result<usize, i32> {
        trace!("file_mode_set to {:#x}", mode);

        let c_path = self.file_path.clone();
        let c_path = c_path.into_raw();
        let r = unsafe { ext4_mode_set(c_path, mode) };
        unsafe {
            drop(CString::from_raw(c_path));
        }
        if r != EOK as i32 {
            error!("ext4_mode_set: rc = {}", r);
            return Err(r);
        }
        Ok(EOK as usize)
    }

    pub fn file_type_get(&mut self) -> InodeTypes {
        let mode = self.file_mode_get().unwrap();
        // 0o777 (octal) == rwxrwxrwx
        // if filetype == EXT4_DE_SYMLINK;
        // mode = 0777;
        // mode |= EXT4_INODE_MODE_SOFTLINK;
        let cal: u32 = 0o777;
        let types = mode & (!cal);
        let itypes = match types {
            0x1000 => InodeTypes::EXT4_INODE_MODE_FIFO,
            0x2000 => InodeTypes::EXT4_INODE_MODE_CHARDEV,
            0x4000 => InodeTypes::EXT4_INODE_MODE_DIRECTORY,
            0x6000 => InodeTypes::EXT4_INODE_MODE_BLOCKDEV,
            0x8000 => InodeTypes::EXT4_INODE_MODE_FILE,
            0xA000 => InodeTypes::EXT4_INODE_MODE_SOFTLINK,
            0xC000 => InodeTypes::EXT4_INODE_MODE_SOCKET,
            0xF000 => InodeTypes::EXT4_INODE_MODE_TYPE_MASK,
            _ => {
                warn!("Unknown inode mode type {:x}", types);
                InodeTypes::EXT4_INODE_MODE_FILE
            }
        };
        trace!("Inode mode types: {:?}", itypes);

        itypes
    }

    /********* DIRECTORY OPERATION *********/

    /// Create new directory
    pub fn dir_mk(&mut self, path: &str) -> Result<usize, i32> {
        trace!("directory create: {}", path);
        let c_path = CString::new(path).expect("CString::new failed");
        let c_path = c_path.into_raw();

        let r = unsafe { ext4_dir_mk(c_path) };
        unsafe {
            drop(CString::from_raw(c_path));
        }
        if r != EOK as i32 {
            error!("ext4_dir_mk: rc = {}", r);
            return Err(r);
        }
        Ok(EOK as usize)
    }

    /// Rename/move directory
    pub fn dir_mv(&mut self, path: &str, new_path: &str) -> Result<usize, i32> {
        trace!("directory move from {} to {}", path, new_path);

        let c_path = CString::new(path).expect("CString::new failed");
        let c_path = c_path.into_raw();
        let c_new_path = CString::new(new_path).expect("CString::new failed");
        let c_new_path = c_new_path.into_raw();

        let r = unsafe { ext4_dir_mv(c_path, c_new_path) };
        unsafe {
            drop(CString::from_raw(c_path));
            drop(CString::from_raw(c_new_path));
        }
        if r != EOK as i32 {
            error!("ext4_dir_mv: rc = {}", r);
            return Err(r);
        }
        Ok(EOK as usize)
    }

    /// Recursive directory remove
    pub fn dir_rm(&mut self, path: &str) -> Result<usize, i32> {
        trace!("directory recursive remove: {}", path);

        let c_path = CString::new(path).expect("CString::new failed");
        let c_path = c_path.into_raw();

        let r = unsafe { ext4_dir_rm(c_path) };
        unsafe {
            drop(CString::from_raw(c_path));
        }
        if (r != EOK as i32) && (r != ENOENT as i32) {
            error!("ext4_fremove ext4_dir_rm: rc = {}", r);
            return Err(r);
        }
        Ok(EOK as usize)
    }

    pub fn lwext4_dir_entries(&self) -> Result<(Vec<Vec<u8>>, Vec<InodeTypes>), i32> {
        if self.this_type != InodeTypes::EXT4_DE_DIR {
            return Err(-1);
        }

        let c_path = self.file_path.clone();
        let c_path = c_path.into_raw();
        let mut d: ext4_dir = unsafe { core::mem::zeroed() };

        let mut name: Vec<Vec<u8>> = Vec::new();
        let mut inode_type: Vec<InodeTypes> = Vec::new();

        //info!("ls {}", str::from_utf8(path).unwrap());
        unsafe {
            ext4_dir_open(&mut d, c_path);
            drop(CString::from_raw(c_path));

            let mut de = ext4_dir_entry_next(&mut d);
            while !de.is_null() {
                let dentry = &(*de);
                let len = dentry.name_length as usize;

                let mut sss: [u8; 255] = [0; 255];
                sss[..len].copy_from_slice(&dentry.name[..len]);
                sss[len] = 0;

                trace!(
                    "  {} {}",
                    dentry.inode_type,
                    core::str::from_utf8(&sss).unwrap()
                );
                /*   let mut dname: Vec<u8> =
                    Vec::from_raw_parts(&mut dentry.name as *mut u8, len, len + 1);
                dname.push(0);
                */
                name.push(sss[..(len + 1)].to_vec());
                inode_type.push((dentry.inode_type as usize).into());

                de = ext4_dir_entry_next(&mut d);
            }
            ext4_dir_close(&mut d);
        }

        Ok((name, inode_type))
    }
}

/*
pub enum OpenFlags {
O_RDONLY = 0,
O_WRONLY = 0x1,
O_RDWR = 0x2,
O_CREAT = 0x40,
O_TRUNC = 0x200,
O_APPEND = 0x400,
}
*/

#[derive(PartialEq, Clone, Debug)]
pub enum InodeTypes {
    // Inode type, Directory entry types.
    EXT4_DE_UNKNOWN = 0,
    EXT4_DE_REG_FILE = 1,
    EXT4_DE_DIR = 2,
    EXT4_DE_CHRDEV = 3,
    EXT4_DE_BLKDEV = 4,
    EXT4_DE_FIFO = 5,
    EXT4_DE_SOCK = 6,
    EXT4_DE_SYMLINK = 7,

    // Inode mode
    EXT4_INODE_MODE_FIFO = 0x1000,
    EXT4_INODE_MODE_CHARDEV = 0x2000,
    EXT4_INODE_MODE_DIRECTORY = 0x4000,
    EXT4_INODE_MODE_BLOCKDEV = 0x6000,
    EXT4_INODE_MODE_FILE = 0x8000,
    EXT4_INODE_MODE_SOFTLINK = 0xA000,
    EXT4_INODE_MODE_SOCKET = 0xC000,
    EXT4_INODE_MODE_TYPE_MASK = 0xF000,
}

impl From<usize> for InodeTypes {
    fn from(num: usize) -> InodeTypes {
        match num {
            0 => InodeTypes::EXT4_DE_UNKNOWN,
            1 => InodeTypes::EXT4_DE_REG_FILE,
            2 => InodeTypes::EXT4_DE_DIR,
            3 => InodeTypes::EXT4_DE_CHRDEV,
            4 => InodeTypes::EXT4_DE_BLKDEV,
            5 => InodeTypes::EXT4_DE_FIFO,
            6 => InodeTypes::EXT4_DE_SOCK,
            7 => InodeTypes::EXT4_DE_SYMLINK,
            0x1000 => InodeTypes::EXT4_INODE_MODE_FIFO,
            0x2000 => InodeTypes::EXT4_INODE_MODE_CHARDEV,
            0x4000 => InodeTypes::EXT4_INODE_MODE_DIRECTORY,
            0x6000 => InodeTypes::EXT4_INODE_MODE_BLOCKDEV,
            0x8000 => InodeTypes::EXT4_INODE_MODE_FILE,
            0xA000 => InodeTypes::EXT4_INODE_MODE_SOFTLINK,
            0xC000 => InodeTypes::EXT4_INODE_MODE_SOCKET,
            0xF000 => InodeTypes::EXT4_INODE_MODE_TYPE_MASK,
            _ => {
                warn!("Unknown ext4 inode type: {}", num);
                InodeTypes::EXT4_DE_UNKNOWN
            }
        }
    }
}
