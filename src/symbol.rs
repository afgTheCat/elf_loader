use crate::{arch::ElfSymbol, dynamic::ElfDynamic};
use elf::abi::{SHN_UNDEF, STB_GLOBAL, STB_GNU_UNIQUE, STB_WEAK};

#[derive(Clone)]
struct ElfGnuHash {
    pub nbucket: u32,
    pub table_start_idx: u32,
    pub nshift: u32,
    pub blooms: &'static [usize],
    pub buckets: *const u32,
    pub chains: *const u32,
}

impl ElfGnuHash {
    #[inline]
    pub(crate) unsafe fn parse(ptr: *const u8) -> ElfGnuHash {
        struct Reader {
            ptr: *const u8,
        }

        impl Reader {
            #[inline]
            fn new(ptr: *const u8) -> Reader {
                Reader { ptr }
            }

            #[inline]
            fn read<T>(&mut self) -> T {
                unsafe {
                    let value = self.ptr.cast::<T>().read();
                    self.ptr = self.ptr.add(core::mem::size_of::<T>());
                    value
                }
            }

            #[inline]
            //字节为单位
            fn add(&mut self, count: usize) {
                self.ptr = unsafe { self.ptr.add(count) };
            }

            #[inline]
            fn as_ptr(&self) -> *const u8 {
                self.ptr
            }
        }

        let mut reader = Reader::new(ptr);

        let nbucket: u32 = reader.read();
        let table_start_idx: u32 = reader.read();
        let nbloom: u32 = reader.read();
        let nshift: u32 = reader.read();
        let blooms_ptr = reader.as_ptr() as *const usize;
        let blooms = unsafe { core::slice::from_raw_parts(blooms_ptr, nbloom as _) };
        let bloom_size = nbloom as usize * core::mem::size_of::<usize>();
        reader.add(bloom_size);
        let buckets = reader.as_ptr() as _;
        reader.add(nbucket as usize * core::mem::size_of::<u32>());
        let chains = reader.as_ptr() as _;
        ElfGnuHash {
            nbucket,
            blooms,
            nshift,
            table_start_idx,
            buckets,
            chains,
        }
    }

    #[inline]
    pub(crate) fn gnu_hash(name: &[u8]) -> u32 {
        let mut hash = 5381u32;
        for byte in name {
            hash = hash.wrapping_mul(33).wrapping_add(u32::from(*byte));
        }
        hash
    }
}

pub(crate) struct ElfStringTable {
    data: *const u8,
}

impl ElfStringTable {
    const fn new(data: *const u8) -> Self {
        ElfStringTable { data }
    }

    pub(crate) fn get(&self, offset: usize) -> &'static str {
        unsafe {
            let start = self.data.add(offset);
            let mut end = start;
            while end.read() != 0 {
                end = end.add(1)
            }
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(
                start,
                end as usize - start as usize,
            ))
        }
    }
}

/// Symbol table of elf file.
pub struct SymbolTable {
    /// .gnu.hash
    hashtab: ElfGnuHash,
    /// .dynsym
    symtab: *const ElfSymbol,
    /// .dynstr
    strtab: ElfStringTable,
    #[cfg(feature = "version")]
    /// .gnu.version
    pub(crate) version: Option<super::version::ELFVersion>,
}

/// Symbol specific information, including symbol name and version name.
pub struct SymbolInfo<'a> {
    pub(crate) name: &'a str,
    #[cfg(feature = "version")]
    version: Option<super::version::SymbolVersion<'a>>,
}

impl<'a> SymbolInfo<'a> {
    pub(crate) const fn new(name: &'a str) -> Self {
        SymbolInfo {
            name,
            #[cfg(feature = "version")]
            version: None,
        }
    }

    #[cfg(feature = "version")]
    pub(crate) fn new_with_version(name: &'a str, version: &'a str) -> Self {
        SymbolInfo {
            name,
            version: Some(crate::version::SymbolVersion::new(version)),
        }
    }

    /// Gets the name of the symbol.
    #[inline]
    pub fn symbol_name(&self) -> &str {
        &self.name
    }
}

impl SymbolTable {
    pub(crate) fn new(dynamic: &ElfDynamic) -> Self {
        let hashtab = unsafe { ElfGnuHash::parse(dynamic.hashtab as *const u8) };
        let symtab = dynamic.symtab as *const ElfSymbol;
        let strtab = ElfStringTable::new(dynamic.strtab as *const u8);
        #[cfg(feature = "version")]
        let version = super::version::ELFVersion::new(
            dynamic.version_idx,
            dynamic.verneed,
            dynamic.verdef,
            &strtab,
        );
        SymbolTable {
            hashtab,
            symtab,
            strtab,
            #[cfg(feature = "version")]
            version,
        }
    }

    pub(crate) fn strtab(&self) -> &ElfStringTable {
        &self.strtab
    }

    /// Use the symbol specific information to get the symbol in the symbol table
    pub fn lookup(&self, symbol: &SymbolInfo) -> Option<&ElfSymbol> {
        let hash = ElfGnuHash::gnu_hash(symbol.name.as_bytes());
        let bloom_width: u32 = 8 * size_of::<usize>() as u32;
        let bloom_idx = (hash / (bloom_width)) as usize % self.hashtab.blooms.len();
        let filter = self.hashtab.blooms[bloom_idx] as u64;
        if filter & (1 << (hash % bloom_width)) == 0 {
            return None;
        }
        let hash2 = hash >> self.hashtab.nshift;
        if filter & (1 << (hash2 % bloom_width)) == 0 {
            return None;
        }
        let table_start_idx = self.hashtab.table_start_idx as usize;
        let chain_start_idx = unsafe {
            self.hashtab
                .buckets
                .add((hash as usize) % self.hashtab.nbucket as usize)
                .read()
        } as usize;
        if chain_start_idx == 0 {
            return None;
        }
        let mut dynsym_idx = chain_start_idx;
        let mut cur_chain = unsafe { self.hashtab.chains.add(dynsym_idx - table_start_idx) };
        let mut cur_symbol_ptr = unsafe { self.symtab.add(dynsym_idx) };
        loop {
            let chain_hash = unsafe { cur_chain.read() };
            if hash | 1 == chain_hash | 1 {
                let cur_symbol = unsafe { &*cur_symbol_ptr };
                let sym_name = self.strtab.get(cur_symbol.st_name as usize);
                #[cfg(feature = "version")]
                if sym_name == symbol.name && self.check_match(dynsym_idx, &symbol.version) {
                    return Some(cur_symbol);
                }
                #[cfg(not(feature = "version"))]
                if sym_name == symbol.name {
                    return Some(cur_symbol);
                }
            }
            if chain_hash & 1 != 0 {
                break;
            }
            cur_chain = unsafe { cur_chain.add(1) };
            cur_symbol_ptr = unsafe { cur_symbol_ptr.add(1) };
            dynsym_idx += 1;
        }
        None
    }

    /// Use the symbol specific information to get the symbol which can be used for relocation in the symbol table
    #[inline]
    pub fn lookup_filter(&self, symbol: &SymbolInfo) -> Option<&ElfSymbol> {
        if let Some(sym) = self.lookup(symbol) {
            if (sym.st_shndx != SHN_UNDEF)
                && (1 << (sym.st_info >> 4)
                    & (1 << STB_GLOBAL | 1 << STB_WEAK | 1 << STB_GNU_UNIQUE)
                    != 0)
            {
                return Some(sym);
            }
        }
        None
    }

    /// Use the symbol index to get the symbols in the symbol table.
    pub fn symbol_idx(&self, idx: usize) -> (&ElfSymbol, SymbolInfo) {
        let symbol = unsafe { &*self.symtab.add(idx) };
        let name = self.strtab.get(symbol.st_name as usize);
        (symbol, SymbolInfo {
            name,
            #[cfg(feature = "version")]
            version: self.get_requirement(idx),
        })
    }
}
