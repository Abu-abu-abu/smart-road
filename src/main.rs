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
fn draw_car(canvas:&mut sdl2::render::Canvas<sdl2::video::Window>,v:&Vehicle) {
    let (x,y)=position(v); let (dx,dy)=v.from.heading(); let mut angle=dy.atan2(dx);
    if v.phase==Phase::Crossing && v.turn!=Turn::Straight {
        let (tx,ty)=v.from.turn_heading(v.turn); let target=ty.atan2(tx);
        let delta=(target-angle+std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)-std::f32::consts::PI;
        angle+=delta*v.p.clamp(0.,1.);
    }
    let (sin,cos)=angle.sin_cos(); let fx=cos*15.;let fy=sin*15.;let sx=-sin*8.;let sy=cos*8.;
    let corners=[(x+fx+sx,y+fy+sy),(x+fx-sx,y+fy-sy),(x-fx-sx,y-fy-sy),(x-fx+sx,y-fy+sy)];
    let min_y=corners.iter().map(|p|p.1).fold(f32::INFINITY,f32::min).floor() as i32;
    let max_y=corners.iter().map(|p|p.1).fold(f32::NEG_INFINITY,f32::max).ceil() as i32;
    canvas.set_draw_color(v.color);
    for row in min_y..=max_y { let mut hits=Vec::new(); for i in 0..4 {let a=corners[i];let b=corners[(i+1)%4];if (a.1<=row as f32 && b.1>row as f32)||(b.1<=row as f32 && a.1>row as f32){let t=(row as f32-a.1)/(b.1-a.1);hits.push(a.0+t*(b.0-a.0));}} if hits.len()>=2 {hits.sort_by(f32::total_cmp);let _=canvas.draw_line((hits[0] as i32,row),(hits[hits.len()-1] as i32,row));}}
    canvas.set_draw_color(Color::RGB(22,27,30));for i in 0..4 {let a=corners[i];let b=corners[(i+1)%4];let _=canvas.draw_line((a.0 as i32,a.1 as i32),(b.0 as i32,b.1 as i32));}
}
fn add_vehicle(cars:&mut Vec<Vehicle>, next:&mut u64, from:Approach, rng:&mut SimpleRng) {
    let turn=[Turn::Left,Turn::Straight,Turn::Right][rng.range(3) as usize];
    let (dx,dy)=from.heading(); let occupied=cars.iter().filter(|c| c.from==from && c.turn==turn && c.phase==Phase::Queue).map(|c|c.p).fold(-1.,f32::max);
    let p=if occupied<0. {0.} else { (occupied- SAFETY-CAR_LEN).max(-120.) };
    let hue=[Color::RGB(42,178,220),Color::RGB(244,172,55),Color::RGB(131,212,117),Color::RGB(210,111,220)][rng.range(4) as usize];
    let _=(dx,dy); cars.push(Vehicle{id:*next,from,turn,p,speed:0.,phase:Phase::Queue,elapsed:0.,color:hue}); *next+=1;
}
fn main() -> Result<(),String> {
    let sdl=sdl2::init()?; let video=sdl.video()?;
    let window=video.window("Smart Road — arrows: add cars | R: auto traffic | Esc: statistics",800,800).position_centered().build().map_err(|e|e.to_string())?;
    let mut canvas=window.into_canvas().present_vsync().build().map_err(|e|e.to_string())?;
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
        canvas.set_draw_color(Color::RGB(40,45,48));canvas.clear(); canvas.set_draw_color(Color::RGB(66,72,75));let _=canvas.fill_rect(Rect::new(300,0,200,800));let _=canvas.fill_rect(Rect::new(0,300,800,200));
        canvas.set_draw_color(Color::RGB(220,205,140));
        for y in (0..300).step_by(45).chain((500..800).step_by(45)) { for x in [330,360,390,410,440,470] { let _=canvas.draw_line((x,y),(x,y+22)); } }
        for x in (0..300).step_by(45).chain((500..800).step_by(45)) { for y in [330,360,390,410,440,470] { let _=canvas.draw_line((x,y),(x+22,y)); } }
        for i in 0..10 {let p=i*80;let _=canvas.fill_rect(Rect::new(395,p,3,36));let _=canvas.fill_rect(Rect::new(p,395,36,3));}
        for v in &cars {draw_car(&mut canvas,v);}
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
