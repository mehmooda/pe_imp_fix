use std::fs;

use object::{read::archive::ArchiveFile, Object, ObjectSection, ReadCache};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args();

    let _ = args.next().ok_or("Missing program name")?;
    let archive_path = args.next().ok_or("Missing archive")?;
    let new_location = args.next();
    let ar_location = args.next();

    if args.next().is_some() {
        Err("unhandled extra arguments")?;
    }

    let file = fs::File::open(&archive_path);
    if file.is_err() {
        eprintln!("Unable to open {}", &archive_path);
    }
    let file = ReadCache::new(file?);
    let archive = ArchiveFile::parse(&file)?;

    let mut object_name = None;
    let mut coff_data = None;
    for x in archive.members() {
        let object = x?;

        let name = object.name();
        let data = object.data(&file)?;

        let coff = object::coff::CoffFile::parse(data)?;

        let section = coff.section_by_name(".idata$7");

        if let Some(s) = section {
            let x = s.data()?;
            if x.len() > 4 || x != [0, 0, 0, 0] {
                if new_location.is_none() {
                    println!(
                        "{}",
                        String::from_utf8(
                            x[0..x
                                .iter()
                                .position(|b| *b == 0)
                                .ok_or(".idata doesn't contain a null terminated string")?]
                                .to_vec()
                        )
                        .unwrap()
                    );
                    return Ok(());
                }

                coff_data = Some(data.to_vec());
                object_name = Some(String::from_utf8(name.to_vec()).unwrap());
                break;
            }
        }
    }

    let object_name = object_name.unwrap();
    let mut coff = coff_data.unwrap();
    let new_location = new_location.unwrap();

    let offset = if let Some((offset, _)) = coff.windows(8).enumerate().find(|x| x.1 == b".idata$7")
    {
        offset
    } else {
        unreachable!();
    };

    let eof = coff.len() as u32;
    <byteorder::LittleEndian as byteorder::ByteOrder>::write_u32(
        &mut coff[offset + 20..],
        eof as u32,
    );
    <byteorder::LittleEndian as byteorder::ByteOrder>::write_u32(
        &mut coff[offset + 16..],
        new_location.len() as u32 + 1,
    );
    coff.extend(new_location.as_bytes());
    coff.push(0);

    fs::write(&object_name, coff).unwrap();

    let mut run = std::process::Command::new(ar_location.as_deref().unwrap_or("ar"))
        .arg("r")
        .arg(archive_path)
        .arg(&object_name)
        .spawn()
        .or(Err("Unable to Execute ar command"))
        .unwrap();

    if !run.wait()?.success() {
        fs::remove_file(object_name)?;
        std::io::copy(&mut run.stdout.take().unwrap(), &mut std::io::stdout()).unwrap();
        std::io::copy(&mut run.stderr.take().unwrap(), &mut std::io::stderr()).unwrap();

        Err("Archive failed").unwrap()
    }

    Ok(())
}
