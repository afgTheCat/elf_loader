//! Contains content related to the CPU instruction set
use elf::{
    abi::{
        SHN_UNDEF, STB_GLOBAL, STB_GNU_UNIQUE, STB_LOCAL, STB_WEAK, STT_COMMON, STT_FUNC,
        STT_GNU_IFUNC, STT_NOTYPE, STT_OBJECT, STT_TLS,
    },
    file::Class,
};

cfg_if::cfg_if! {
    if #[cfg(target_arch = "x86_64")]{
        mod x86_64;
        pub use x86_64::*;
    }else if #[cfg(target_arch = "riscv64")]{
        mod riscv64;
        pub use riscv64::*;
    }else if #[cfg(target_arch="aarch64")]{
        mod aarch64;
        pub use aarch64::*;
    }
}

pub const REL_NONE: u32 = 0;
const OK_BINDS: usize = 1 << STB_GLOBAL | 1 << STB_WEAK | 1 << STB_GNU_UNIQUE;
const OK_TYPES: usize = 1 << STT_NOTYPE
    | 1 << STT_OBJECT
    | 1 << STT_FUNC
    | 1 << STT_COMMON
    | 1 << STT_TLS
    | 1 << STT_GNU_IFUNC;

cfg_if::cfg_if! {
    if #[cfg(target_pointer_width = "64")]{
        pub(crate) const E_CLASS: Class = Class::ELF64;
        pub type Phdr = elf::segment::Elf64_Phdr;
        pub type Dyn = elf::dynamic::Elf64_Dyn;
        pub(crate) type Rela = elf::relocation::Elf64_Rela;
        pub(crate) type Sym = elf::symbol::Elf64_Sym;
        pub(crate) const REL_MASK: usize = 0xFFFFFFFF;
        pub(crate) const REL_BIT: usize = 32;
        pub(crate) const PHDR_SIZE: usize = core::mem::size_of::<elf::segment::Elf64_Phdr>();
        pub(crate) const EHDR_SIZE: usize = core::mem::size_of::<elf::file::Elf64_Ehdr>();
    }else{
        pub(crate) const E_CLASS: Class = Class::ELF32;
        pub type Phdr = elf::segment::Elf32_Phdr;
        pub type Dyn = elf::dynamic::Elf32_Dyn;
        pub(crate) type Rela = elf::relocation::Elf32_Rela;
        pub(crate) type Sym = elf::symbol::Elf32_Sym;
        pub(crate) const REL_MASK: usize = 0xFF;
        pub(crate) const REL_BIT: usize = 8;
        pub(crate) const PHDR_SIZE: usize = core::mem::size_of::<elf::segment::Elf32_Phdr>();
        pub(crate) const EHDR_SIZE: usize = core::mem::size_of::<elf::file::Elf32_Ehdr>();
    }
}

#[repr(C)]
pub struct ElfRela {
    rela: Rela,
}

impl ElfRela {
    #[inline]
    pub fn r_type(&self) -> usize {
        self.rela.r_info as usize & REL_MASK
    }

    #[inline]
    pub fn r_symbol(&self) -> usize {
        self.rela.r_info as usize >> REL_BIT
    }

    #[inline]
    pub fn r_offset(&self) -> usize {
        self.rela.r_offset as usize
    }

    #[inline]
    pub fn r_addend(&self) -> usize {
        self.rela.r_addend as usize
    }
}

#[repr(C)]
pub struct ElfSymbol {
    sym: Sym,
}

impl ElfSymbol {
    #[inline]
    pub fn st_value(&self) -> usize {
        self.sym.st_value as usize
    }

    /// STB_* define constants for the ELF Symbol's st_bind (encoded in the st_info field)
    #[inline]
    pub fn st_bind(&self) -> u8 {
        self.sym.st_info >> 4
    }

    /// STT_* define constants for the ELF Symbol's st_type (encoded in the st_info field).
    #[inline]
    pub fn st_type(&self) -> u8 {
        self.sym.st_info & 0xf
    }

    #[inline]
    pub fn st_shndx(&self) -> usize {
        self.sym.st_shndx as usize
    }

    #[inline]
    pub fn st_name(&self) -> usize {
        self.sym.st_name as usize
    }

    #[inline]
    pub fn st_size(&self) -> usize {
        self.sym.st_size as usize
    }

	#[inline]
    pub fn st_other(&self) -> u8 {
        self.sym.st_other
    }

    #[inline]
    pub fn is_undef(&self) -> bool {
        self.st_shndx() == SHN_UNDEF as usize
    }

    #[inline]
    pub fn is_ok_bind(&self) -> bool {
        (1 << self.st_bind()) & OK_BINDS != 0
    }

    #[inline]
    pub fn is_ok_type(&self) -> bool {
        (1 << self.st_type()) & OK_TYPES != 0
    }

    #[inline]
    pub fn is_local(&self) -> bool {
        self.st_bind() == STB_LOCAL
    }
}
