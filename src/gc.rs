use std::collections::HashSet;
use std::ptr::NonNull;

pub trait Trace {
    fn trace(&self, _marked: &mut HashSet<*const GcBoxHeader>);
}

pub struct GcBoxHeader {
    pub marked: bool,
    pub next: Option<NonNull<GcBox<dyn Trace>>>,
}

#[repr(C)]
pub struct GcBox<T: Trace + ?Sized> {
    pub header: GcBoxHeader,
    pub data: T,
}

pub struct Gc<T: Trace + ?Sized> {
    pub ptr: NonNull<GcBox<T>>,
}

impl<T: Trace + ?Sized> std::fmt::Debug for Gc<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Gc({:p})", self.ptr.as_ptr())
    }
}

impl<T: Trace + ?Sized> Clone for Gc<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Trace + ?Sized> Copy for Gc<T> {}

unsafe impl<T: Trace + ?Sized> Send for Gc<T> {}
unsafe impl<T: Trace + ?Sized> Sync for Gc<T> {}

impl<T: Trace + ?Sized> std::ops::Deref for Gc<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &self.ptr.as_ref().data }
    }
}

pub struct GcHeap {
    all_objects: Option<NonNull<GcBox<dyn Trace>>>,
}

impl GcHeap {
    pub fn new() -> Self {
        Self { all_objects: None }
    }

    pub fn allocate<T: Trace + 'static>(&mut self, data: T) -> Gc<T> {
        let gc_box = Box::new(GcBox {
            header: GcBoxHeader {
                marked: false,
                next: self.all_objects,
            },
            data,
        });
        let ptr = unsafe { NonNull::new_unchecked(Box::into_raw(gc_box)) };
        self.all_objects = Some(ptr as NonNull<GcBox<dyn Trace>>);
        Gc { ptr }
    }

    pub unsafe fn collect(&mut self, roots: &[&dyn Trace]) {
        let mut marked = HashSet::new();
        for root in roots {
            root.trace(&mut marked);
        }

        // Sweep would go here
        // For this initial port, we'll keep it as a stub or minimal implementation
    }
}

impl Trace for String {
    fn trace(&self, _marked: &mut HashSet<*const GcBoxHeader>) {}
}
