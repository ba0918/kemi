use super::*;

#[test]
fn captured_pipe_returns_every_byte() {
    let bytes = vec![b'x'; 2 * 1024 * 1024];
    let mut pipe = process::CapturedPipe::new(std::io::Cursor::new(bytes.clone()));
    let mut captured = Vec::new();
    pipe.read_to_end(&mut captured).unwrap();
    assert_eq!(captured, bytes);
}

#[cfg(unix)]
#[test]
fn both_process_pipes_are_drained_while_waiting_for_large_output_to_finish() {
    let mut child = StopOnDrop(
        Command::new("sh")
            .args([
                "-c",
                "head -c 2097152 /dev/zero; head -c 2097152 /dev/zero >&2",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut stdout = process::CapturedPipe::new(child.0.stdout.take().unwrap());
    let mut stderr = process::CapturedPipe::new(child.0.stderr.take().unwrap());
    assert!(
        process::wait(&mut child.0, std::time::Duration::from_secs(20))
            .unwrap()
            .success()
    );
    for pipe in [&mut stdout, &mut stderr] {
        let mut bytes = Vec::new();
        pipe.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, vec![0; 2 * 1024 * 1024]);
    }
}

#[cfg(unix)]
#[test]
fn a_process_that_does_not_exit_is_bounded_and_reaped_by_its_owner() {
    let mut child = StopOnDrop(
        Command::new("sh")
            .args(["-c", "exec sleep 60"])
            .spawn()
            .unwrap(),
    );
    let result = process::wait(&mut child.0, std::time::Duration::from_millis(30));
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::TimedOut);
    stop(&mut child.0);
    assert!(child.0.try_wait().unwrap().is_some());
}

#[cfg(unix)]
#[test]
fn input_for_a_process_that_exits_without_reading_it_is_not_an_error() {
    let mut child = StopOnDrop(Command::new("true").stdin(Stdio::piped()).spawn().unwrap());
    let stdin = child.0.stdin.take().unwrap();
    assert!(
        process::wait(&mut child.0, std::time::Duration::from_secs(20))
            .unwrap()
            .success()
    );

    process::feed(stdin, vec![b'x'; 1024 * 1024])
        .finish()
        .unwrap();
}
