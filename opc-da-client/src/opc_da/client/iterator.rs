use crate::opc_da::{
    def::ItemAttributes,
    utils::{RemoteArray, RemotePointer, TryToLocal as _},
};

const MAX_CACHE_SIZE: usize = 16;

/// Iterator over COM GUIDs from IEnumGUID.  
///
/// # Safety  
/// This struct wraps a COM interface and must be used according to COM rules.  
pub struct GuidIterator {
    inner: windows::Win32::System::Com::IEnumGUID,
    cache: Box<[windows::core::GUID; MAX_CACHE_SIZE]>,
    index: u32,
    count: u32,
    done: bool,
}

impl GuidIterator {
    /// Creates a new iterator from a COM interface.  
    pub fn new(inner: windows::Win32::System::Com::IEnumGUID) -> Self {
        Self {
            inner,
            cache: Box::from([windows::core::GUID::zeroed(); MAX_CACHE_SIZE]),
            index: MAX_CACHE_SIZE as u32,
            count: 0,
            done: false,
        }
    }
}

impl Iterator for GuidIterator {
    type Item = windows::core::Result<windows::core::GUID>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        if self.index >= self.count {
            let code = unsafe {
                self.inner
                    .Next(self.cache.as_mut_slice(), Some(&mut self.count))
            };

            if code.is_ok() {
                if self.count == 0 {
                    self.done = true;
                    return None;
                }

                self.index = 0;
            } else {
                self.done = true;
                return Some(Err(windows::core::Error::new(
                    code,
                    "Failed to get next GUID",
                )));
            }
        }

        let current = self.cache[self.index as usize];
        self.index += 1;
        Some(Ok(current))
    }
}

pub struct StringIterator {
    inner: windows::Win32::System::Com::IEnumString,
    cache: Box<[windows::core::PWSTR; MAX_CACHE_SIZE]>,
    index: u32,
    count: u32,
    done: bool,
}

impl StringIterator {
    pub fn new(inner: windows::Win32::System::Com::IEnumString) -> Self {
        Self {
            inner,
            cache: Box::new([windows::core::PWSTR::null(); MAX_CACHE_SIZE]),
            index: MAX_CACHE_SIZE as u32,
            count: 0,
            done: false,
        }
    }
}

impl Iterator for StringIterator {
    type Item = windows::core::Result<String>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        if self.index >= self.count {
            let code = unsafe {
                self.inner
                    .Next(self.cache.as_mut_slice(), Some(&mut self.count))
            };

            if code.is_ok() {
                if self.count == 0 {
                    self.done = true;
                    return None;
                }

                self.index = 0;
            } else {
                self.done = true;
                return Some(Err(windows::core::Error::new(
                    code,
                    "Failed to get next string",
                )));
            }
        }

        let current = RemotePointer::from(self.cache[self.index as usize]);
        self.index += 1;
        Some(current.try_into())
    }
}

pub struct GroupIterator<Group: TryFrom<windows::core::IUnknown, Error = windows::core::Error>> {
    inner: windows::Win32::System::Com::IEnumUnknown,
    cache: Box<[Option<windows::core::IUnknown>; MAX_CACHE_SIZE]>,
    index: u32,
    count: u32,
    done: bool,
    _mark: std::marker::PhantomData<Group>,
}

impl<Group: TryFrom<windows::core::IUnknown, Error = windows::core::Error>> GroupIterator<Group> {
    pub fn new(inner: windows::Win32::System::Com::IEnumUnknown) -> Self {
        Self {
            inner,
            cache: Box::from([const { None }; MAX_CACHE_SIZE]),
            index: MAX_CACHE_SIZE as u32,
            count: 0,
            done: false,
            _mark: std::marker::PhantomData,
        }
    }
}

impl<Group: TryFrom<windows::core::IUnknown, Error = windows::core::Error>> Iterator
    for GroupIterator<Group>
{
    type Item = windows::core::Result<Group>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        if self.index >= self.count {
            let code = unsafe {
                self.inner
                    .Next(self.cache.as_mut_slice(), Some(&mut self.count))
            };

            if code.is_ok() {
                if self.count == 0 {
                    self.done = true;
                    return None;
                }

                self.index = 0;
            } else {
                self.done = true;
                return Some(Err(windows::core::Error::new(
                    code,
                    "Failed to get next group",
                )));
            }
        }

        let current = self.cache[self.index as usize].take();
        self.index += 1;
        Some(match current {
            Some(group) => group.try_into(),
            None => Err(windows::core::Error::new(
                windows::Win32::Foundation::E_POINTER,
                "Failed to get group, returned null",
            )),
        })
    }
}

// for crate::bindings::da::IEnumOPCItemAttributes
pub struct ItemAttributeIterator {
    inner: crate::bindings::da::IEnumOPCItemAttributes,
    cache: RemoteArray<crate::bindings::da::tagOPCITEMATTRIBUTES>,
    index: u32,
    done: bool,
}

impl ItemAttributeIterator {
    pub fn new(inner: crate::bindings::da::IEnumOPCItemAttributes) -> Self {
        Self {
            inner,
            cache: RemoteArray::empty(),
            index: 0,
            done: false,
        }
    }
}

impl Iterator for ItemAttributeIterator {
    type Item = windows::core::Result<ItemAttributes>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        if self.index >= self.cache.len() {
            let mut attrs = RemoteArray::new(MAX_CACHE_SIZE as u32);

            let result = unsafe {
                self.inner.Next(
                    MAX_CACHE_SIZE as u32,
                    attrs.as_mut_ptr(),
                    attrs.as_mut_len_ptr(),
                )
            };

            match result {
                Ok(_) => {
                    if attrs.is_empty() {
                        self.done = true;
                        return None;
                    }

                    self.cache = attrs;
                    self.index = 0;
                }
                Err(err) => {
                    self.done = true;
                    return Some(Err(err));
                }
            }
        }

        let current = self.cache.as_slice()[self.index as usize].try_to_local();
        self.index += 1;
        Some(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Com::{IEnumString, IEnumString_Impl};
    use windows::core::{PWSTR, implement};

    #[implement(IEnumString)]
    struct MockEnumString {
        items: Vec<String>,
        index: std::sync::atomic::AtomicUsize,
    }

    impl IEnumString_Impl for MockEnumString_Impl {
        fn Next(
            &self,
            celt: u32,
            rgelt: *mut PWSTR,
            pceltfetched: *mut u32,
        ) -> windows::core::HRESULT {
            let mut fetched = 0;
            let index = self.index.load(std::sync::atomic::Ordering::Relaxed);
            let rgelt = unsafe { std::slice::from_raw_parts_mut(rgelt, celt as usize) };

            for i in 0..celt as usize {
                if index + i < self.items.len() {
                    let s = &self.items[index + i];
                    let mut w: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
                    let ptr = unsafe { windows::Win32::System::Com::CoTaskMemAlloc(w.len() * 2) };
                    unsafe { std::ptr::copy_nonoverlapping(w.as_ptr(), ptr as *mut u16, w.len()) };
                    rgelt[i] = PWSTR(ptr as *mut u16);
                    fetched += 1;
                } else {
                    break;
                }
            }

            self.index
                .store(index + fetched, std::sync::atomic::Ordering::Relaxed);

            if !pceltfetched.is_null() {
                unsafe { *pceltfetched = fetched as u32 };
            }

            if fetched == celt as usize {
                windows::Win32::Foundation::S_OK.into()
            } else {
                windows::Win32::Foundation::S_FALSE.into()
            }
        }
        fn Skip(&self, _celt: u32) -> windows::core::HRESULT {
            windows::Win32::Foundation::E_NOTIMPL.into()
        }
        fn Reset(&self) -> windows::core::Result<()> {
            self.index.store(0, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        }
        fn Clone(&self) -> windows::core::Result<IEnumString> {
            Err(windows::core::Error::from_hresult(
                windows::Win32::Foundation::E_NOTIMPL,
            ))
        }
    }

    #[test]
    fn test_string_iterator_no_phantom_errors() {
        let items = vec![
            "Item1".to_string(),
            "Item2".to_string(),
            "Item3".to_string(),
        ];

        let mock_enum: IEnumString = MockEnumString {
            items: items.clone(),
            index: std::sync::atomic::AtomicUsize::new(0),
        }
        .into();

        let iter = StringIterator::new(mock_enum);

        let mut results = Vec::new();
        for item in iter {
            // Verify no E_POINTER error is yielded
            let value = item.expect("Expected OK value, got phantom error");
            results.push(value);
        }

        assert_eq!(results, items);
    }
}
