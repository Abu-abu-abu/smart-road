use sdl2::{event::Event, keyboard::Keycode, messagebox::MessageBoxFlag, pixels::Color, rect::Rect};
use std::collections::HashSet;
use std::time::{Duration, Instant};

const SAFETY: f32 = 34.0;
const CAR_LEN: f32 = 26.0;
const STOP_LINE: f32 = 230.0;
const CENTER: f32 = 400.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Approach { North, East, South, West }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Turn { Left, Straight, Right }
impl Approach {
    fn heading(self) -> (f32, f32) { match self { Self::North => (0.,1.), Self::East => (-1.,0.), Self::South => (0.,-1.), Self::West => (1.,0.) } }
    fn start(self, turn:Turn) -> (f32,f32) {
        let (dx,dy)=self.heading(); let lateral=(-dy,dx); let offset=lane_offset(turn);
        let (x,y)=match self { Self::North=>(CENTER,40.), Self::South=>(CENTER,760.), Self::West=>(40.,CENTER), Self::East=>(760.,CENTER) };
        (x+lateral.0*offset,y+lateral.1*offset)
    }
    fn turn_heading(self, turn:Turn)->(f32,f32) {
        let (dx,dy)=self.heading();
        match turn { Turn::Straight=>(dx,dy), Turn::Left=>(dy,-dx), Turn::Right=>(-dy,dx) }
    }
    fn exit(self, t: Turn) -> (f32,f32) {
        let (dx,dy)=self.turn_heading(t); let lateral=(-dy,dx); let offset=lane_offset(t);
        (CENTER+dx*360.+lateral.0*offset,CENTER+dy*360.+lateral.1*offset)
    }
}
fn lane_offset(turn:Turn)->f32 { match turn { Turn::Right=>85.,Turn::Straight=>55.,Turn::Left=>25. } }
#[derive(Clone, Copy, Debug, PartialEq, Eq)] enum Phase { Queue, Crossing }
struct Vehicle { id:u64, from:Approach, turn:Turn, p:f32, speed:f32, phase:Phase, elapsed:f32, color:Color }
#[derive(Default)] struct Stats { passed:u64, min_speed:f32, max_speed:f32, min_time:f32, max_time:f32, close_calls:u64 }
impl Stats {
    fn record_speed(&mut self, speed:f32) { self.min_speed=self.min_speed.min(speed); self.max_speed=self.max_speed.max(speed); }
    fn record_pass(&mut self, time:f32) { self.passed+=1; self.min_time=self.min_time.min(time); self.max_time=self.max_time.max(time); }
}
struct SimpleRng(u64);
impl SimpleRng { fn next(&mut self)->u32 { self.0=self.0.wrapping_mul(6364136223846793005).wrapping_add(1); (self.0>>32) as u32 } fn range(&mut self,n:u32)->u32 { self.next()%n } }
fn dist(a:(f32,f32),b:(f32,f32))->f32 { ((a.0-b.0).powi(2)+(a.1-b.1).powi(2)).sqrt() }
fn position(v:&Vehicle)->(f32,f32) {
    let lane_start=v.from.start(v.turn); let end=v.from.exit(v.turn); let (dx,dy)=v.from.heading();
    if v.phase==Phase::Queue { return (lane_start.0+dx*v.p,lane_start.1+dy*v.p); }
    let lateral=(-dy,dx); let offset=lane_offset(v.turn);
    let start=(CENTER-dx*60.+lateral.0*offset,CENTER-dy*60.+lateral.1*offset); let t=v.p.clamp(0.,1.);
    if v.turn==Turn::Straight { return (start.0+(end.0-start.0)*t,start.1+(end.1-start.1)*t); }
    let (ex,ey)=v.from.turn_heading(v.turn);
    let c1=(start.0+dx*50.,start.1+dy*50.); let c2=(end.0-ex*80.,end.1-ey*80.); let u=1.-t;
    (u*u*u*start.0+3.*u*u*t*c1.0+3.*u*t*t*c2.0+t*t*t*end.0,u*u*u*start.1+3.*u*u*t*c1.1+3.*u*t*t*c2.1+t*t*t*end.1)
}
fn velocity(distance:f32,time:f32)->f32 { if time<=0. {0.} else {distance/time} }
fn should_stop_for_gap(gap:f32)->bool { gap<=SAFETY }
fn reservation_owner(cars:&[Vehicle])->Option<u64> {
    cars.iter().find(|v|v.phase==Phase::Crossing).map(|v|v.id).or_else(||cars.iter().filter(|v|v.phase==Phase::Queue && v.p>=STOP_LINE).map(|v|v.id).min())
}
fn should_wait_for_reservation(owner:Option<u64>,vehicle_id:u64,progress:f32)->bool { owner.is_some() && owner!=Some(vehicle_id) && progress>=STOP_LINE }
type Cv = sdl2::render::Canvas<sdl2::video::Window>;

fn shade(c: Color, f: f32) -> Color {
    let m = |v: u8| ((v as f32) * f).clamp(0., 255.) as u8;
    Color::RGB(m(c.r), m(c.g), m(c.b))
}

/// Scanline polygon fill (works with alpha because every pixel is drawn once).
fn fill_poly(canvas: &mut Cv, pts: &[(f32, f32)]) {
    let n = pts.len();
    if n < 3 { return; }
    let min_y = pts.iter().map(|p| p.1).fold(f32::INFINITY, f32::min).floor() as i32;
    let max_y = pts.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max).ceil() as i32;
    let mut hits: Vec<f32> = Vec::with_capacity(n);
    for row in min_y..=max_y {
        let ry = row as f32 + 0.5;
        hits.clear();
        for i in 0..n {
            let a = pts[i];
            let b = pts[(i + 1) % n];
            if (a.1 <= ry && b.1 > ry) || (b.1 <= ry && a.1 > ry) {
                let t = (ry - a.1) / (b.1 - a.1);
                hits.push(a.0 + t * (b.0 - a.0));
            }
        }
        hits.sort_by(f32::total_cmp);
        let mut k = 0;
        while k + 1 < hits.len() {
            let _ = canvas.draw_line((hits[k].round() as i32, row), (hits[k + 1].round() as i32, row));
            k += 2;
        }
    }
}

fn fill_circle(canvas: &mut Cv, cx: i32, cy: i32, r: i32, color: Color) {
    canvas.set_draw_color(color);
    for dy in -r..=r {
        let w = ((r * r - dy * dy) as f32).sqrt() as i32;
        let _ = canvas.fill_rect(Rect::new(cx - w, cy + dy, (2 * w + 1) as u32, 1));
    }
}

fn thick_line(canvas: &mut Cv, a: (f32, f32), b: (f32, f32), w: f32) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    let (nx, ny) = (-dy / len * w * 0.5, dx / len * w * 0.5);
    fill_poly(canvas, &[(a.0 + nx, a.1 + ny), (b.0 + nx, b.1 + ny), (b.0 - nx, b.1 - ny), (a.0 - nx, a.1 - ny)]);
}

fn pose(v: &Vehicle) -> (f32, f32, f32) {
    let (x, y) = position(v);
    let (dx, dy) = v.from.heading();
    let mut angle = dy.atan2(dx);
    if v.phase == Phase::Crossing && v.turn != Turn::Straight {
        let (tx, ty) = v.from.turn_heading(v.turn);
        let target = ty.atan2(tx);
        let delta = (target - angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        angle += delta * v.p.clamp(0., 1.);
    }
    (x, y, angle)
}

/// Car-local coordinates: `u` runs along the heading, `w` across the car.
fn local_frame(x: f32, y: f32, angle: f32) -> impl Fn(f32, f32) -> (f32, f32) {
    let (sin, cos) = angle.sin_cos();
    move |u, w| (x + cos * u - sin * w, y + sin * u + cos * w)
}

/// Pass 1: things that lie on the road under every car (headlight beams, brake glow, shadow).
fn draw_car_under(canvas: &mut Cv, v: &Vehicle) {
    let (x, y, ang) = pose(v);
    let pt = local_frame(x, y, ang);
    canvas.set_draw_color(Color::RGBA(255, 240, 190, 18));
    fill_poly(canvas, &[pt(14., -6.), pt(14., 6.), pt(58., 18.), pt(58., -18.)]);
    canvas.set_draw_color(Color::RGBA(255, 240, 190, 22));
    fill_poly(canvas, &[pt(14., -5.), pt(14., 5.), pt(34., 9.), pt(34., -9.)]);
    if v.speed < 0.5 {
        canvas.set_draw_color(Color::RGBA(255, 60, 50, 34));
        fill_poly(canvas, &[pt(-15., -6.), pt(-15., 6.), pt(-27., 10.), pt(-27., -10.)]);
    }
    let sh = |u: f32, w: f32| { let p = pt(u, w); (p.0 + 3., p.1 + 4.) };
    canvas.set_draw_color(Color::RGBA(0, 0, 0, 90));
    fill_poly(canvas, &[sh(15., -8.), sh(15., 8.), sh(-15., 8.), sh(-15., -8.)]);
}

/// Pass 2: the car itself.
fn draw_car(canvas: &mut Cv, v: &Vehicle) {
    let (x, y, ang) = pose(v);
    let pt = local_frame(x, y, ang);
    let stopped = v.speed < 0.5;

    let body = [pt(-15., -6.), pt(-13., -8.), pt(12., -8.), pt(15., -5.), pt(15., 5.), pt(12., 8.), pt(-13., 8.), pt(-15., 6.)];
    canvas.set_draw_color(v.color);
    fill_poly(canvas, &body);
    canvas.set_draw_color(shade(v.color, 0.55));
    for i in 0..body.len() {
        let a = body[i];
        let b = body[(i + 1) % body.len()];
        let _ = canvas.draw_line((a.0.round() as i32, a.1.round() as i32), (b.0.round() as i32, b.1.round() as i32));
    }

    // glass, then a lighter roof on top of it
    canvas.set_draw_color(Color::RGB(30, 42, 56));
    fill_poly(canvas, &[pt(10., -5.5), pt(10., 5.5), pt(-9., 6.), pt(-9., -6.)]);
    canvas.set_draw_color(Color::RGBA(170, 205, 230, 80));
    fill_poly(canvas, &[pt(9., -4.5), pt(9., -1.5), pt(6., -1.5), pt(6., -4.5)]);
    canvas.set_draw_color(shade(v.color, 1.15));
    fill_poly(canvas, &[pt(4., -5.), pt(4., 5.), pt(-6., 5.5), pt(-6., -5.5)]);

    // headlights and tail lights (brake lights are brighter while stopped)
    let tail = if stopped { Color::RGB(255, 64, 52) } else { Color::RGB(150, 34, 34) };
    for s in [-1., 1.] {
        canvas.set_draw_color(Color::RGB(255, 238, 176));
        fill_poly(canvas, &[pt(12., s * 3.), pt(15., s * 3.), pt(15., s * 5.), pt(12., s * 6.)]);
        canvas.set_draw_color(tail);
        fill_poly(canvas, &[pt(-15., s * 3.), pt(-12., s * 3.), pt(-12., s * 6.), pt(-15., s * 5.)]);
    }

    // blinking turn signal on the side of the planned turn
    let side = match v.turn { Turn::Left => Some(-1.), Turn::Right => Some(1.), Turn::Straight => None };
    let visible = match v.phase { Phase::Queue => v.p > 40., Phase::Crossing => v.p < 0.55 };
    let blink_on = ((v.elapsed * 3.) as i32) % 2 == 0;
    if let Some(s) = side {
        if visible && blink_on {
            canvas.set_draw_color(Color::RGB(255, 176, 32));
            for u in [13., -13.] {
                fill_poly(canvas, &[pt(u - 1.5, s * 5.5), pt(u + 1.5, s * 5.5), pt(u + 1.5, s * 8.5), pt(u - 1.5, s * 8.5)]);
            }
        }
    }
}

fn draw_scene(c: &mut Cv) {
    // lawn with mown stripes (only the four corners are visible)
    c.set_draw_color(Color::RGB(40, 78, 58));
    c.clear();
    c.set_draw_color(Color::RGB(45, 87, 64));
    for ty in 0..25i32 {
        for tx in 0..25i32 {
            if (tx + ty) % 2 == 0 && (tx < 9 || tx >= 16) && (ty < 9 || ty >= 16) {
                let _ = c.fill_rect(Rect::new(tx * 32, ty * 32, 32, 32));
            }
        }
    }

    // trees seen from above: shadow, dark crown, lighter layers, highlight
    let trees: [(i32, i32, i32); 24] = [
        (52, 58, 22), (150, 38, 15), (238, 82, 13), (38, 172, 15), (130, 150, 23), (222, 236, 16),
        (748, 66, 20), (640, 44, 16), (560, 110, 14), (764, 190, 14), (662, 160, 21), (580, 246, 15),
        (56, 744, 19), (160, 764, 14), (246, 700, 15), (36, 620, 13), (128, 650, 22), (224, 568, 17),
        (744, 736, 21), (650, 758, 15), (566, 704, 14), (766, 604, 15), (668, 640, 23), (580, 574, 16),
    ];
    for &(x, y, r) in trees.iter() {
        fill_circle(c, x + 4, y + 6, r, Color::RGBA(0, 0, 0, 60));
        fill_circle(c, x, y, r, Color::RGB(27, 66, 46));
        fill_circle(c, x - r / 8, y - r / 8, r * 3 / 4, Color::RGB(36, 88, 58));
        fill_circle(c, x - r / 4, y - r / 4, r / 2, Color::RGB(54, 116, 74));
        fill_circle(c, x - r / 3, y - r / 3, (r / 5).max(2), Color::RGB(88, 150, 98));
    }

    // sidewalks with paving joints and curbs
    c.set_draw_color(Color::RGB(166, 166, 158));
    let _ = c.fill_rect(Rect::new(288, 0, 224, 800));
    let _ = c.fill_rect(Rect::new(0, 288, 800, 224));
    c.set_draw_color(Color::RGB(142, 144, 138));
    for i in (0..800).step_by(24) {
        let _ = c.draw_line((288, i), (299, i));
        let _ = c.draw_line((500, i), (511, i));
        let _ = c.draw_line((i, 288), (i, 299));
        let _ = c.draw_line((i, 500), (i, 511));
    }
    for &(a, b) in [(0, 288), (512, 800)].iter() {
        c.set_draw_color(Color::RGB(208, 208, 200));
        let _ = c.draw_line((288, a), (288, b));
        let _ = c.draw_line((511, a), (511, b));
        let _ = c.draw_line((a, 288), (b, 288));
        let _ = c.draw_line((a, 511), (b, 511));
        c.set_draw_color(Color::RGB(78, 82, 84));
        let _ = c.draw_line((299, a), (299, b));
        let _ = c.draw_line((500, a), (500, b));
        let _ = c.draw_line((a, 299), (b, 299));
        let _ = c.draw_line((a, 500), (b, 500));
    }

    // asphalt with a little grain
    c.set_draw_color(Color::RGB(43, 49, 55));
    let _ = c.fill_rect(Rect::new(300, 0, 200, 800));
    let _ = c.fill_rect(Rect::new(0, 300, 800, 200));
    c.set_draw_color(Color::RGB(47, 53, 59));
    let _ = c.fill_rect(Rect::new(300, 300, 200, 200));
    let mut s: u64 = 0x2545F4914F6CDD1D;
    for i in 0..500 {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let x = ((s >> 33) % 800) as i32;
        let y = ((s >> 13) % 800) as i32;
        if (300..500).contains(&x) || (300..500).contains(&y) {
            c.set_draw_color(if i % 2 == 0 { Color::RGB(58, 64, 70) } else { Color::RGB(34, 39, 44) });
            let _ = c.fill_rect(Rect::new(x, y, 2, 1));
        }
    }

    // median strip with double yellow lines
    let median: [(i32, i32, u32, u32); 4] = [(391, 0, 18, 300), (391, 500, 18, 300), (0, 391, 300, 18), (500, 391, 300, 18)];
    c.set_draw_color(Color::RGB(54, 60, 66));
    for &(x, y, w, h) in median.iter() { let _ = c.fill_rect(Rect::new(x, y, w, h)); }
    let yellow: [(i32, i32, u32, u32); 8] = [
        (389, 0, 2, 300), (409, 0, 2, 300), (389, 500, 2, 300), (409, 500, 2, 300),
        (0, 389, 300, 2), (0, 409, 300, 2), (500, 389, 300, 2), (500, 409, 300, 2),
    ];
    c.set_draw_color(Color::RGB(236, 190, 64));
    for &(x, y, w, h) in yellow.iter() { let _ = c.fill_rect(Rect::new(x, y, w, h)); }

    // dashed white lane dividers
    c.set_draw_color(Color::RGB(204, 208, 206));
    for a in (0..300).step_by(40).chain((500..800).step_by(40)) {
        for &b in [329, 359, 439, 469].iter() {
            let _ = c.fill_rect(Rect::new(b, a, 2, 20));
            let _ = c.fill_rect(Rect::new(a, b, 20, 2));
        }
    }

    // stop lines
    c.set_draw_color(Color::RGB(228, 230, 226));
    let _ = c.fill_rect(Rect::new(301, 287, 88, 4));
    let _ = c.fill_rect(Rect::new(411, 509, 88, 4));
    let _ = c.fill_rect(Rect::new(287, 411, 4, 88));
    let _ = c.fill_rect(Rect::new(509, 301, 4, 88));

    // painted lane arrows (left / straight / right) for every approach
    c.set_draw_color(Color::RGB(138, 144, 146));
    for &ap in [Approach::North, Approach::East, Approach::South, Approach::West].iter() {
        for &t in [Turn::Left, Turn::Straight, Turn::Right].iter() {
            let (hx, hy) = ap.heading();
            let (dx, dy) = ap.turn_heading(t);
            let st = ap.start(t);
            let p = (st.0 + hx * 190., st.1 + hy * 190.);
            let tail = (p.0 - hx * 14., p.1 - hy * 14.);
            let q = (p.0 + dx * 6., p.1 + dy * 6.);
            let tip = (p.0 + dx * 12., p.1 + dy * 12.);
            let (nx, ny) = (-dy * 5., dx * 5.);
            thick_line(c, tail, p, 3.);
            thick_line(c, p, q, 3.);
            fill_poly(c, &[tip, (q.0 + nx, q.1 + ny), (q.0 - nx, q.1 - ny)]);
        }
    }

    // zebra crossings
    c.set_draw_color(Color::RGB(214, 216, 208));
    for i in 0..10 {
        let p = 305 + i * 20;
        let _ = c.fill_rect(Rect::new(p, 303, 10, 14));
        let _ = c.fill_rect(Rect::new(p, 483, 10, 14));
    }
    for i in 0..8 {
        let p = 325 + i * 20;
        let _ = c.fill_rect(Rect::new(303, p, 14, 10));
        let _ = c.fill_rect(Rect::new(483, p, 14, 10));
    }
}

fn draw_vignette(c: &mut Cv) {
    for i in 0..28i32 {
        let a = (70. - i as f32 * 2.4).max(0.) as u8;
        c.set_draw_color(Color::RGBA(0, 0, 0, a));
        let _ = c.draw_rect(Rect::new(i, i, (800 - 2 * i) as u32, (800 - 2 * i) as u32));
    }
}
fn glyph_rows(c:char)->[u8;5] { match c.to_ascii_uppercase() {
    'A'=>[2,5,7,5,5],'B'=>[6,5,6,5,6],'C'=>[3,4,4,4,3],'D'=>[6,5,5,5,6],'E'=>[7,4,6,4,7],'F'=>[7,4,6,4,4],
    'G'=>[3,4,5,5,3],'H'=>[5,5,7,5,5],'I'=>[7,2,2,2,7],'J'=>[1,1,1,5,2],'K'=>[5,5,6,5,5],'L'=>[4,4,4,4,7],
    'M'=>[5,7,7,5,5],'N'=>[5,7,7,7,5],'O'=>[2,5,5,5,2],'P'=>[6,5,6,4,4],'Q'=>[2,5,5,7,3],'R'=>[6,5,6,5,5],
    'S'=>[3,4,2,1,6],'T'=>[7,2,2,2,2],'U'=>[5,5,5,5,7],'V'=>[5,5,5,5,2],'W'=>[5,5,7,7,5],'X'=>[5,5,2,5,5],
    'Y'=>[5,5,2,2,2],'Z'=>[7,1,2,4,7],'0'=>[7,5,5,5,7],'1'=>[2,6,2,2,7],'2'=>[6,1,2,4,7],'3'=>[6,1,2,1,6],
    '4'=>[5,5,7,1,1],'5'=>[7,4,6,1,6],'6'=>[3,4,6,5,2],'7'=>[7,1,2,2,2],'8'=>[2,5,2,5,2],'9'=>[2,5,3,1,6],
    ':'=>[0,2,0,2,0],'-'=>[0,0,7,0,0],'.'=>[0,0,0,0,2],' '=>[0;5],_=>[7,1,2,0,2],
} }
fn draw_label(canvas:&mut sdl2::render::Canvas<sdl2::video::Window>,text:&str,x:i32,y:i32,scale:i32,color:Color) {
    canvas.set_draw_color(color);let mut cursor=x;
    for ch in text.chars(){let rows=glyph_rows(ch);for (row,bits) in rows.iter().enumerate(){for col in 0..3{if *bits&(1<<(2-col))!=0{let _=canvas.fill_rect(Rect::new(cursor+col*scale,y+row as i32*scale,scale as u32,scale as u32));}}}cursor+=4*scale;}
}
fn text_width(s: &str, scale: i32) -> i32 { s.chars().count() as i32 * 4 * scale - scale }
fn draw_text(canvas: &mut Cv, s: &str, x: i32, y: i32, scale: i32, color: Color) {
    let o = if scale >= 3 { 2 } else { 1 };
    draw_label(canvas, s, x + o, y + o, scale, Color::RGBA(0, 0, 0, 150));
    draw_label(canvas, s, x, y, scale, color);
}
fn draw_panel(canvas: &mut Cv, x: i32, y: i32, w: u32, h: u32, accent: Color) {
    canvas.set_draw_color(Color::RGBA(0, 0, 0, 70));
    let _ = canvas.fill_rect(Rect::new(x + 3, y + 4, w, h));
    canvas.set_draw_color(Color::RGBA(12, 18, 24, 215));
    let _ = canvas.fill_rect(Rect::new(x, y, w, h));
    canvas.set_draw_color(Color::RGBA(255, 255, 255, 38));
    let _ = canvas.draw_rect(Rect::new(x, y, w, h));
    canvas.set_draw_color(accent);
    let _ = canvas.fill_rect(Rect::new(x, y, w, 3));
}
fn draw_keycap(canvas: &mut Cv, label: &str, x: i32, y: i32) {
    let w = (text_width(label, 2) + 10) as u32;
    canvas.set_draw_color(Color::RGB(6, 10, 14));
    let _ = canvas.fill_rect(Rect::new(x, y + 2, w, 18));
    canvas.set_draw_color(Color::RGB(66, 82, 90));
    let _ = canvas.fill_rect(Rect::new(x, y, w, 18));
    canvas.set_draw_color(Color::RGB(112, 132, 140));
    let _ = canvas.fill_rect(Rect::new(x, y, w, 1));
    draw_label(canvas, label, x + 5, y + 4, 2, Color::RGB(235, 245, 243));
}
fn draw_toggle(canvas: &mut Cv, x: i32, y: i32, on: bool) {
    canvas.set_draw_color(if on { Color::RGB(73, 202, 190) } else { Color::RGB(66, 78, 84) });
    let _ = canvas.fill_rect(Rect::new(x, y, 40, 18));
    canvas.set_draw_color(Color::RGB(240, 248, 246));
    let _ = canvas.fill_rect(Rect::new(x + if on { 24 } else { 2 }, y + 2, 14, 14));
}
fn draw_hud(canvas: &mut Cv, passed: u64, active: usize, close_calls: u64, auto: bool) {
    let ink = Color::RGB(235, 245, 243);
    let dim = Color::RGB(139, 164, 166);
    let teal = Color::RGB(73, 202, 190);
    let amber = Color::RGB(245, 185, 87);
    let coral = Color::RGB(235, 145, 132);

    // title
    draw_panel(canvas, 16, 16, 250, 70, teal);
    draw_text(canvas, "SMART ROAD", 30, 28, 3, ink);
    draw_text(canvas, "AUTONOMOUS INTERSECTION", 30, 60, 2, dim);
    fill_circle(canvas, 246, 36, 5, if auto { teal } else { Color::RGB(90, 104, 110) });

    // live counters
    draw_panel(canvas, 500, 16, 284, 92, amber);
    draw_text(canvas, "PASSED", 516, 30, 2, dim);
    draw_text(canvas, &format!("{}", passed), 516, 56, 4, amber);
    draw_text(canvas, "ACTIVE", 604, 30, 2, dim);
    draw_text(canvas, &format!("{}", active), 604, 56, 4, ink);
    draw_text(canvas, "CLOSE", 692, 30, 2, dim);
    draw_text(canvas, &format!("{}", close_calls), 692, 56, 4, coral);
    canvas.set_draw_color(Color::RGBA(255, 255, 255, 28));
    let _ = canvas.draw_line((596, 34), (596, 96));
    let _ = canvas.draw_line((684, 34), (684, 96));

    // controls
    draw_panel(canvas, 16, 700, 232, 84, teal);
    let rows = [("ARROWS", "ADD CAR"), ("R", "AUTO TRAFFIC"), ("ESC", "FINAL REPORT")];
    for (i, (key, what)) in rows.iter().enumerate() {
        let y = 712 + i as i32 * 22;
        draw_keycap(canvas, key, 30, y);
        draw_text(canvas, what, 100, y + 4, 2, ink);
    }

    // auto-traffic status
    draw_panel(canvas, 584, 720, 200, 64, if auto { teal } else { Color::RGB(90, 104, 110) });
    draw_text(canvas, "AUTO TRAFFIC", 598, 732, 2, dim);
    draw_toggle(canvas, 598, 754, auto);
    draw_text(canvas, if auto { "ON" } else { "OFF" }, 648, 758, 2, if auto { teal } else { dim });
}
fn add_vehicle(cars:&mut Vec<Vehicle>, next:&mut u64, from:Approach, rng:&mut SimpleRng) {
    let turn=[Turn::Left,Turn::Straight,Turn::Right][rng.range(3) as usize];
    let (dx,dy)=from.heading(); let occupied=cars.iter().filter(|c| c.from==from && c.turn==turn && c.phase==Phase::Queue).map(|c|c.p).fold(-1.,f32::max);
    let p=if occupied<0. {0.} else { (occupied- SAFETY-CAR_LEN).max(-120.) };
    let hue=[Color::RGB(38,166,154),Color::RGB(245,166,35),Color::RGB(235,96,86),Color::RGB(140,110,230),Color::RGB(66,153,225),Color::RGB(150,205,85)][rng.range(6) as usize];
    let _=(dx,dy); cars.push(Vehicle{id:*next,from,turn,p,speed:0.,phase:Phase::Queue,elapsed:0.,color:hue}); *next+=1;
}
fn main() -> Result<(),String> {
    let sdl=sdl2::init()?; let video=sdl.video()?;
    let window=video.window("Smart Road — arrows: add cars | R: auto traffic | Esc: statistics",800,800).position_centered().build().map_err(|e|e.to_string())?;
    let mut canvas=window.into_canvas().present_vsync().build().map_err(|e|e.to_string())?;
    canvas.set_blend_mode(sdl2::render::BlendMode::Blend);
    let mut events=sdl.event_pump()?; let mut cars:Vec<Vehicle>=Vec::new(); let mut rng=SimpleRng(0x534d415254524f41);
    let mut next=1; let mut stats=Stats{min_speed:f32::INFINITY,min_time:f32::INFINITY,..Stats::default()}; let mut running=true; let mut show_stats=false; let mut counted_close_pairs=HashSet::new();
    let mut last=Instant::now(); let mut spawn_clock=0.; let mut auto=false;
    while running { let dt=last.elapsed().as_secs_f32().min(0.05); last=Instant::now(); spawn_clock+=dt;
        for e in events.poll_iter() { match e { Event::Quit{..}=>{show_stats=true;running=false}, Event::KeyDown{keycode:Some(k),repeat:false,..}=>match k { Keycode::Escape=>{show_stats=true;running=false},Keycode::R if !show_stats=>{auto=!auto},Keycode::Up if !show_stats=>add_vehicle(&mut cars,&mut next,Approach::South,&mut rng),Keycode::Down if !show_stats=>add_vehicle(&mut cars,&mut next,Approach::North,&mut rng),Keycode::Left if !show_stats=>add_vehicle(&mut cars,&mut next,Approach::East,&mut rng),Keycode::Right if !show_stats=>add_vehicle(&mut cars,&mut next,Approach::West,&mut rng),_=>{} },_=>{} } }
        if auto && !show_stats && spawn_clock>=0.9 { spawn_clock=0.; let from=[Approach::North,Approach::East,Approach::South,Approach::West][rng.range(4) as usize]; add_vehicle(&mut cars,&mut next,from,&mut rng); }
        // A single reservation owner at a time is conservative: it prevents every crossing conflict.
        let reservation=reservation_owner(&cars);
        let mut queue_ids:Vec<u64>=cars.iter().filter(|v|v.phase==Phase::Queue).map(|v|v.id).collect(); queue_ids.sort_unstable();
        for id in queue_ids { let idx=cars.iter().position(|v|v.id==id).unwrap(); let front=cars.iter().enumerate().filter(|(j,v)|*j!=idx && v.from==cars[idx].from && v.turn==cars[idx].turn && v.phase==Phase::Queue && v.p>cars[idx].p).map(|(_,v)|v.p).fold(f32::INFINITY,f32::min);
            let gap=front-cars[idx].p-CAR_LEN; let safe=!should_stop_for_gap(gap);
            if cars[idx].p>=0. { cars[idx].elapsed+=dt; }
            let conflict=should_wait_for_reservation(reservation,id,cars[idx].p);
            if conflict || !safe { cars[idx].speed=0.; } else { cars[idx].speed=if cars[idx].p<70. {58.} else {94.}; cars[idx].p+=cars[idx].speed*dt; }
            if reservation.is_some() && reservation!=Some(id) && cars[idx].p>STOP_LINE { cars[idx].p=STOP_LINE; }
            if cars[idx].p>=300. { cars[idx].phase=Phase::Crossing; cars[idx].p=0.; }
        }
        for v in cars.iter_mut().filter(|v|v.phase==Phase::Crossing) { v.elapsed+=dt; let path=if v.turn==Turn::Straight {420.} else {390.}; v.speed=if v.turn==Turn::Straight {82.} else {55.}; let d=velocity(v.speed,1.)*dt; v.p+=d/path; }
        for v in &cars { stats.record_speed(v.speed); }
        let done:Vec<u64>=cars.iter().filter(|v|v.phase==Phase::Crossing && v.p>=1.).map(|v|v.id).collect();
        for id in done { if let Some(i)=cars.iter().position(|v|v.id==id) { let v=cars.remove(i);stats.record_pass(v.elapsed); } }
        // Detect dangerously close pairs (used as a visible metric as well as a safety check).
        for i in 0..cars.len(){for j in i+1..cars.len(){let pair=(cars[i].id.min(cars[j].id),cars[i].id.max(cars[j].id));if dist(position(&cars[i]),position(&cars[j]))<CAR_LEN+SAFETY && counted_close_pairs.insert(pair){stats.close_calls+=1;}}}
        draw_scene(&mut canvas);
        for v in &cars {draw_car_under(&mut canvas,v);}
        for v in &cars {draw_car(&mut canvas,v);}
        draw_vignette(&mut canvas);
        draw_hud(&mut canvas,stats.passed,cars.len(),stats.close_calls,auto);
        canvas.present();std::thread::sleep(Duration::from_millis(8));
    }
    if show_stats { let report=format!("Vehicles passed: {}\nFastest speed: {:.1} px/s\nSlowest speed: {:.1} px/s\nLongest time: {:.2} s\nShortest time: {:.2} s\nClose calls: {}",stats.passed,stats.max_speed,if stats.min_speed.is_finite(){stats.min_speed}else{0.},stats.max_time,if stats.min_time.is_finite(){stats.min_time}else{0.},stats.close_calls); sdl2::messagebox::show_simple_message_box(MessageBoxFlag::INFORMATION,"Smart Road — statistics",&report,None).map_err(|e|e.to_string())?; }
    Ok(())
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn velocity_is_distance_over_time(){assert_eq!(velocity(120.,2.),60.);assert_eq!(velocity(75.,1.5),50.);assert_eq!(velocity(9.,3.),3.);}
    #[test] fn safe_gap_blocks_following_car(){let mut cars=vec![Vehicle{id:1,from:Approach::North,turn:Turn::Straight,p:50.,speed:0.,phase:Phase::Queue,elapsed:0.,color:Color::WHITE},Vehicle{id:2,from:Approach::North,turn:Turn::Straight,p:0.,speed:0.,phase:Phase::Queue,elapsed:0.,color:Color::WHITE}]; let gap=cars[0].p-cars[1].p-CAR_LEN;assert!(gap<=SAFETY);cars[1].speed=if gap>SAFETY{50.}else{0.};assert_eq!(cars[1].speed,0.);cars[1].p=-(SAFETY+CAR_LEN);let gap=cars[0].p-cars[1].p-CAR_LEN;assert!(gap>SAFETY);}
    #[test] fn intersection_reservation_serializes_conflicting_routes(){let cars=vec![Vehicle{id:1,from:Approach::North,turn:Turn::Left,p:0.,speed:60.,phase:Phase::Crossing,elapsed:0.,color:Color::WHITE},Vehicle{id:2,from:Approach::West,turn:Turn::Straight,p:STOP_LINE,speed:0.,phase:Phase::Queue,elapsed:0.,color:Color::WHITE}];let owner=reservation_owner(&cars);assert_eq!(owner,Some(1));assert!(should_wait_for_reservation(owner,2,cars[1].p));assert!(!should_wait_for_reservation(owner,1,300.));}
    #[test] fn statistics_track_passes_and_speed_extrema(){let mut s=Stats{min_speed:f32::INFINITY,min_time:f32::INFINITY,..Stats::default()};s.record_speed(55.);s.record_speed(82.);s.record_pass(4.);s.record_pass(2.);assert_eq!(s.passed,2);assert_eq!(s.min_speed,55.);assert_eq!(s.max_speed,82.);assert_eq!(s.min_time,2.);assert_eq!(s.max_time,4.);}
    #[test] fn equal_or_smaller_gap_requires_a_stop(){assert!(should_stop_for_gap(SAFETY));assert!(should_stop_for_gap(SAFETY-0.1));assert!(!should_stop_for_gap(SAFETY+0.1));}
    #[test] fn active_reservation_holds_other_cars_before_the_stop_line(){assert!(should_wait_for_reservation(Some(1),2,STOP_LINE));assert!(!should_wait_for_reservation(Some(1),1,STOP_LINE));assert!(!should_wait_for_reservation(None,2,STOP_LINE));assert!(300.-(STOP_LINE+94.*0.05)-CAR_LEN>=SAFETY);}
    #[test] fn reservation_selects_one_waiting_vehicle(){let mut cars=vec![Vehicle{id:2,from:Approach::West,turn:Turn::Straight,p:250.,speed:0.,phase:Phase::Queue,elapsed:0.,color:Color::WHITE},Vehicle{id:1,from:Approach::North,turn:Turn::Left,p:260.,speed:0.,phase:Phase::Queue,elapsed:0.,color:Color::WHITE}];assert_eq!(reservation_owner(&cars),Some(1));cars[0].phase=Phase::Crossing;assert_eq!(reservation_owner(&cars),Some(2));}
    #[test] fn each_route_starts_in_a_separate_lane(){let r=Approach::North.start(Turn::Right);let s=Approach::North.start(Turn::Straight);let l=Approach::North.start(Turn::Left);assert_ne!(r,s);assert_ne!(s,l);assert_ne!(r,l);}
    #[test] fn turns_follow_the_drivers_left_and_right(){assert_eq!(Approach::North.turn_heading(Turn::Left),(1.,0.));assert_eq!(Approach::North.turn_heading(Turn::Right),(-1.,0.));}
}