use std::{fs::{File, OpenOptions}, io::Write, process::{Command, Stdio}, time::Instant};
fn main() {
 let a: Vec<String> = std::env::args().collect();
 let dir=&a[1]; let phase=&a[2]; let iteration=&a[3]; let runner=&a[4];
 let log=File::create(format!("{dir}/{phase}-{iteration}-{runner}.log")).unwrap();
 let start=Instant::now();
 let status=Command::new(&a[5]).args(&a[6..]).stdin(Stdio::null()).stdout(log.try_clone().unwrap()).stderr(log).status().unwrap();
 let line=format!("{phase}\t{iteration}\t{runner}\t{:.6}\t{}", start.elapsed().as_secs_f64(),status.code().unwrap_or(-1));
 writeln!(OpenOptions::new().create(true).append(true).open(format!("{dir}/timings.tsv")).unwrap(),"{line}").unwrap();
 println!("{line}");
}
