extern crate appframe;
extern crate bedrock;

use appframe::*;
use std::rc::Rc;
use std::cell::RefCell;

fn main() { GUIApplication::run(App::new()); }

struct App
{
    w: RefCell<Option<Rc<MainWindow>>>
}
impl App
{
    pub fn new() -> Self
    {
        App { w: RefCell::new(None) }
    }
}
impl EventDelegate for App
{
    fn postinit(&self, app: &Rc<GUIApplication<App>>)
    {
        let w = MainWindow::new(app); w.0.borrow().as_ref().unwrap().show();
        *self.w.borrow_mut() = w.into();
    }
}
struct MainWindow(RefCell<Option<NativeWindow<MainWindow>>>);
impl MainWindow
{
    pub fn new(app: &Rc<GUIApplication<App>>) -> Rc<Self>
    {
        let this: Rc<_> = MainWindow(None.into()).into();
        *this.0.borrow_mut() = NativeWindowBuilder::new(512 * 10 / 16, 512, "InfiniteMinesweeper")
            .resizable(false).create_renderable(app, &this).unwrap().into();
        return this;
    }
}
impl WindowEventDelegate for MainWindow
{
    type ClientDelegate = App;
}
