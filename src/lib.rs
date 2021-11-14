// TODO:
//  Replace instances of .unwrap() with proper error handling.
//  Add testing for when a supplied thread function panics.

use std::mem;
use std::sync::mpsc;
use std::thread;
use std::marker::{Send, Sync};
use std::sync::mpsc::{SyncSender, Receiver, RecvError};
use std::thread::JoinHandle;
use std::sync::Arc;
use std::any::Any;
use std::vec::Vec;
use std::collections::HashMap;


pub struct ThreadHandle {
    pub sx: Option<SyncSender<()>>,
    pub join_handle: Option<JoinHandle<()>>,
}

impl Default for ThreadHandle {
    fn default() -> ThreadHandle {
        ThreadHandle {
            sx: None,
            join_handle: None,
        }
    }
}

pub struct EcstaticSystems {
    handles: HashMap<String, Vec<ThreadHandle>>,
}

impl EcstaticSystems {
    pub fn new() -> EcstaticSystems {
        EcstaticSystems { 
            handles: HashMap::new(),
        }
    }

    /// Sends a signal to every possible thread handle amongst all categories.
    pub fn signal_all(&self) {
        for k in self.handles.keys() {
            self.signal(k);
        }
    }

    /// Sends a signal to every thread handle in a category.
    pub fn signal(&self, category: &str) {
        for th in self.handles.get(category).unwrap().iter() {
            th.sx.as_ref().unwrap().send(());
        }
    }

    /// Registers a system which will run on its own thread, but only operates when given a signal through its sender.
    pub fn register_static<'a: 'static, T: Any + Send + Sync>(&mut self, category: &str, data: &'a T, f: fn(Arc<&'a T>)) {
        let th = self.static_system_create(data, f);
        self.lazy_init_category(category);
        self.handles.get_mut(category).unwrap().push(th);
    }

    /// Drops the senders for a thread category and joins each thread in the category.
    pub fn drop_join_category(&mut self, category: &str) {
        if let Some(ths) = self.handles.get_mut(category) {
            for th in ths.iter_mut() {
                let mut handle = mem::take(th);
                mem::drop(handle.sx.take());
                handle.join_handle.take().unwrap().join();
            }
        }
        self.handles.remove(category);
    }

    fn lazy_init_category(&mut self, category: &str) {
        let mut inited = true;
        if let None = self.handles.get_mut(category) {
            inited = false;
        }
        if !inited {
            self.handles.insert(String::from(category), Vec::new());
        }
    }

    fn static_system_create<'a: 'static, T: Any + Send + Sync>(&self, data: &'a T, f: fn(Arc<&'a T>)) -> ThreadHandle {
        let (sx, rx): (SyncSender<()>, Receiver<()>) = mpsc::sync_channel(60);
        let arc_data = Arc::new(data);
        let handle = thread::spawn(move || {
            loop {
                match rx.recv() {
                    Ok(_) => f(arc_data.clone()),
                    Err(_) => break,
                }
            }
        });
        ThreadHandle {
            sx: Some(sx),
            join_handle: Some(handle),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::sync::mpsc::{SyncSender, Receiver, RecvError};
    use std::mem;

    use super::EcstaticSystems;

    #[test]
    fn test_register_system_and_signals() {
        let mut sys = EcstaticSystems::new();
        static ATOMIC: AtomicUsize = AtomicUsize::new(0);

        sys.register_static("testing", &ATOMIC, |x|{ x.fetch_add(1, Ordering::SeqCst); });
        assert!(ATOMIC.load(Ordering::Relaxed) == 0);

        sys.signal_all();
        sys.drop_join_category("testing");

        assert!(ATOMIC.load(Ordering::Relaxed) == 1);
    }

    #[test]
    fn test_register_multiple_systems() {
        let mut sys = EcstaticSystems::new();
        static ATOMIC: AtomicUsize = AtomicUsize::new(0);
        static ATOMIC1: AtomicUsize = AtomicUsize::new(1);
        static ATOMIC2: AtomicUsize = AtomicUsize::new(2);

        sys.register_static("testing", &ATOMIC, |x|{ x.fetch_add(1, Ordering::SeqCst); });
        assert!(ATOMIC.load(Ordering::Relaxed) == 0);
        sys.register_static("testing", &ATOMIC1, |x|{ x.fetch_add(1, Ordering::SeqCst); });
        assert!(ATOMIC1.load(Ordering::Relaxed) == 1);
        sys.signal_all();
        sys.drop_join_category("testing");
        assert!(ATOMIC.load(Ordering::Relaxed) == 1);
        assert!(ATOMIC1.load(Ordering::Relaxed) == 2);


        sys.register_static("testing", &ATOMIC, |x|{ x.fetch_add(1, Ordering::SeqCst); });
        assert!(ATOMIC.load(Ordering::Relaxed) == 1);

        sys.register_static("testing", &ATOMIC1, |x|{ x.fetch_add(1, Ordering::SeqCst); });
        assert!(ATOMIC1.load(Ordering::Relaxed) == 2);

        sys.register_static("testing2", &ATOMIC2, |x|{ x.fetch_add(2, Ordering::SeqCst); });
        assert!(ATOMIC2.load(Ordering::Relaxed) == 2);

        sys.signal_all();
        sys.drop_join_category("testing");
        sys.drop_join_category("testing2");

        assert!(ATOMIC.load(Ordering::Relaxed) == 2);
        assert!(ATOMIC1.load(Ordering::Relaxed) == 3);
        assert!(ATOMIC2.load(Ordering::Relaxed) == 4);
    }

    #[test]
    fn test_signal_category() {
        let mut sys = EcstaticSystems::new();
        static ATOMIC: AtomicUsize = AtomicUsize::new(0);
        static ATOMIC1: AtomicUsize = AtomicUsize::new(2);
        
        sys.register_static("testing", &ATOMIC, |x|{ x.fetch_add(1, Ordering::SeqCst); });
        assert!(ATOMIC.load(Ordering::Relaxed) == 0);
        sys.register_static("testing2", &ATOMIC1, |x|{ x.fetch_add(2, Ordering::SeqCst); });
        assert!(ATOMIC1.load(Ordering::Relaxed) == 2);

        sys.signal("testing");
        sys.drop_join_category("testing");
        sys.drop_join_category("testing2");

        assert!(ATOMIC.load(Ordering::Relaxed) == 1);
        assert!(ATOMIC1.load(Ordering::Relaxed) == 2);
    }

    #[test]
    fn test_static_system_create() {
        let sys = EcstaticSystems::new();
        static ATOMIC: AtomicUsize = AtomicUsize::new(0);

        let th = sys.static_system_create(&ATOMIC, |x|{ x.fetch_add(1, Ordering::SeqCst); });
        assert!(ATOMIC.load(Ordering::Relaxed) == 0);
        for i in 0..100 {
            th.sx.as_ref().unwrap().send(());
        }

        mem::drop(th.sx);               //drop the original sender to send the signal to terminate the thread.         
        th.join_handle.unwrap().join(); //wait for thread to finish the buffered work in the channel. thread is cleaned up afterwards.

        assert!(ATOMIC.load(Ordering::Relaxed) == 100, "Actual: {} ; Expected: {}", ATOMIC.load(Ordering::Relaxed), 100);
    }
}