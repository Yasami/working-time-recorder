use chrono::{Local, SecondsFormat};
use std::env;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const TASK_NAME_NOT_PROVIDED_MSG: &str = "タスク名が提供されていません。";
const TASK_NAME_CONTAINS_NEWLINE_MSG: &str = "タスク名に改行を含めることはできません。";
const FILENAME_NOT_PROVIDED_MSG: &str = "ファイル名が指定されていません";
const HOME_DIR_NOT_FOUND_MSG: &str = "ホームディレクトリが見つかりません。-f または環境変数 WORKING_TIME_RECORD で記録ファイルを指定してください。";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    match execute(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn execute(args: &[String]) -> Result<(), String> {
    let Some(subcommand) = args.get(1) else {
        return Err("No subcommand provided.".to_string());
    };

    match subcommand.as_str() {
        "help" => {
            display_help();
            Ok(())
        }
        "start" => handle_start_command(args),
        "stop" => handle_stop_command(args),
        _ => Err(format!("Invalid subcommand '{subcommand}'.")),
    }
}

fn display_help() {
    println!("Usage:");
    println!("  start <task_name>... [-f <file>]    Start tracking time for a task.");
    println!("                                      Multiple words are joined with spaces.");
    println!("  stop [-f <file>]                    Stop tracking time.");
    println!("  help                                Display this help message.");
}

fn handle_start_command(args: &[String]) -> Result<(), String> {
    let (file_path, remaining_args) = parse_arguments(args)?;
    if remaining_args.is_empty() {
        return Err(TASK_NAME_NOT_PROVIDED_MSG.into());
    }

    let task_name = remaining_args.join(" ");
    // 記録ファイルは1行1レコードなので、改行を含むタスク名は記録を壊さないよう拒否する
    if task_name.contains(['\r', '\n']) {
        return Err(TASK_NAME_CONTAINS_NEWLINE_MSG.into());
    }

    let timestamp = get_current_time();
    let record = format!("{timestamp}\tstart\t{task_name}\n");
    write_to_file(&file_path, &record)
}

fn handle_stop_command(args: &[String]) -> Result<(), String> {
    let (file_path, _remaining_args) = parse_arguments(args)?;
    let timestamp = get_current_time();
    let record = format!("{timestamp}\tstop\t\n");
    write_to_file(&file_path, &record)
}

// 共通の引数処理関数
fn parse_arguments(args: &[String]) -> Result<(PathBuf, Vec<String>), String> {
    parse_arguments_with(args, get_working_time_record_path)
}

/// `-f` が指定されなかったときだけ `default_path` で既定のパスを求める
fn parse_arguments_with(
    args: &[String],
    default_path: impl FnOnce() -> Result<PathBuf, String>,
) -> Result<(PathBuf, Vec<String>), String> {
    let mut file_path = None;
    let mut remaining_args = Vec::new();
    let mut iter = args.iter().skip(2);

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-f" | "--file" => {
                file_path = Some(iter.next().ok_or(FILENAME_NOT_PROVIDED_MSG)?.into());
            }
            _ => remaining_args.push(arg.clone()),
        }
    }

    let file_path = match file_path {
        Some(path) => path,
        None => default_path()?,
    };
    Ok((file_path, remaining_args))
}

fn get_current_time() -> String {
    Local::now().to_rfc3339_opts(SecondsFormat::Secs, false)
}

fn get_working_time_record_path() -> Result<PathBuf, String> {
    resolve_record_path(env::var_os("WORKING_TIME_RECORD"), env::home_dir())
}

/// 環境変数 WORKING_TIME_RECORD の値、未設定ならホームディレクトリの working_time_record.txt
fn resolve_record_path(
    env_value: Option<OsString>,
    home_dir: Option<PathBuf>,
) -> Result<PathBuf, String> {
    if let Some(path) = env_value {
        return Ok(PathBuf::from(path));
    }
    // 空のパスだとカレントディレクトリの相対パスになってしまうので、見つからない扱いにする
    home_dir
        .filter(|home| !home.as_os_str().is_empty())
        .map(|home| home.join("working_time_record.txt"))
        .ok_or_else(|| HOME_DIR_NOT_FOUND_MSG.to_string())
}

fn write_to_file(file_path: &Path, content: &str) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(file_path)
        .map_err(|e| e.to_string())?;
    file.write_all(content.as_bytes())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// 一時ディレクトリの下に置くテスト用の記録ファイル。drop したときに消す
    struct TestFile {
        path: PathBuf,
    }

    impl TestFile {
        // テストは並列に実行され、別の cargo test と同時に動くこともあるので、
        // テスト名とプロセス ID でテストごとに一意なファイル名にする
        fn new(test_name: &str) -> Self {
            let path = env::temp_dir().join(format!(
                "working_time_record_test_{}_{test_name}.txt",
                std::process::id()
            ));
            let _ = fs::remove_file(&path);
            Self { path }
        }

        /// `-f` に渡す引数
        fn arg(&self) -> String {
            self.path
                .to_str()
                .expect("一時ディレクトリのパスが UTF-8 ではありません")
                .to_string()
        }

        fn read(&self) -> String {
            fs::read_to_string(&self.path).unwrap()
        }
    }

    // assert で panic したときも消えるよう、drop で消す
    impl Drop for TestFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    #[test]
    fn test_execute_empty_args() {
        let args = vec!["program_name".to_string()];
        let result = execute(&args);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "No subcommand provided.");
    }

    #[test]
    fn test_execute_help() {
        let args = vec!["program_name".to_string(), "help".to_string()];
        assert!(execute(&args).is_ok());
    }

    #[test]
    fn test_execute_invalid_command() {
        let args = vec!["program_name".to_string(), "invalid".to_string()];
        let result = execute(&args);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid subcommand 'invalid'.");
    }

    #[test]
    fn test_handle_start_command() {
        let test_file = TestFile::new("start");
        let args = vec![
            "program_name".to_string(),
            "start".to_string(),
            "test_task".to_string(),
            "-f".to_string(),
            test_file.arg(),
        ];
        assert!(handle_start_command(&args).is_ok());
        assert!(test_file.read().contains("start\ttest_task"));
    }

    #[test]
    fn test_handle_start_command_missing_task_name() {
        let test_file = TestFile::new("missing_task_name");
        let args = vec![
            "program_name".to_string(),
            "start".to_string(),
            "-f".to_string(),
            test_file.arg(),
        ];
        let result = handle_start_command(&args);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), TASK_NAME_NOT_PROVIDED_MSG);
    }

    #[test]
    fn test_handle_start_command_multiple_words() {
        let test_file = TestFile::new("multiple_words");
        let args = vec![
            "program_name".to_string(),
            "start".to_string(),
            "設計".to_string(),
            "レビュー".to_string(),
            "-f".to_string(),
            test_file.arg(),
        ];
        assert!(handle_start_command(&args).is_ok());
        let content = test_file.read();
        assert_eq!(content.lines().count(), 1);
        assert!(content.ends_with("\tstart\t設計 レビュー\n"));
    }

    #[test]
    fn test_handle_start_command_file_option_in_middle() {
        let test_file = TestFile::new("file_option_in_middle");
        let args = vec![
            "program_name".to_string(),
            "start".to_string(),
            "設計".to_string(),
            "-f".to_string(),
            test_file.arg(),
            "レビュー".to_string(),
        ];
        assert!(handle_start_command(&args).is_ok());
        let content = test_file.read();
        assert_eq!(content.lines().count(), 1);
        assert!(content.ends_with("\tstart\t設計 レビュー\n"));
    }

    #[test]
    fn test_handle_start_command_rejects_newline() {
        let test_file = TestFile::new("rejects_newline");
        for task_name in ["設計\nレビュー", "設計\r\nレビュー", "設計\rレビュー"]
        {
            let args = vec![
                "program_name".to_string(),
                "start".to_string(),
                task_name.to_string(),
                "-f".to_string(),
                test_file.arg(),
            ];
            let result = handle_start_command(&args);
            assert_eq!(result.unwrap_err(), TASK_NAME_CONTAINS_NEWLINE_MSG);
        }
        // 拒否したときは記録ファイルに何も書き込まない
        assert!(!test_file.path.exists());
    }

    #[test]
    fn test_handle_stop_command() {
        let test_file = TestFile::new("stop");
        let args = vec![
            "program_name".to_string(),
            "stop".to_string(),
            "-f".to_string(),
            test_file.arg(),
        ];
        assert!(handle_stop_command(&args).is_ok());
        assert!(test_file.read().contains("stop"));
    }

    #[test]
    fn test_parse_arguments_default_file_path() {
        let args = vec![
            "program_name".to_string(),
            "start".to_string(),
            "test_task".to_string(),
        ];
        // 環境変数 WORKING_TIME_RECORD やホームディレクトリに左右されないよう、既定のパスは引数で渡す。
        // 既定のパスの決め方は resolve_record_path のテストで確認する
        let (file_path, remaining_args) =
            parse_arguments_with(&args, || Ok(PathBuf::from("default_record.txt"))).unwrap();
        assert_eq!(file_path, Path::new("default_record.txt"));
        assert_eq!(remaining_args, vec!["test_task".to_string()]);
    }

    #[test]
    fn test_parse_arguments_custom_file_path() {
        let args = vec![
            "program_name".to_string(),
            "start".to_string(),
            "test_task".to_string(),
            "-f".to_string(),
            "custom_file.txt".to_string(),
        ];
        let (file_path, remaining_args) = parse_arguments(&args).unwrap();
        assert_eq!(file_path, Path::new("custom_file.txt"));
        assert_eq!(remaining_args, vec!["test_task".to_string()]);
    }

    #[test]
    fn test_parse_arguments_file_option_skips_default_path() {
        let args = vec![
            "program_name".to_string(),
            "stop".to_string(),
            "-f".to_string(),
            "custom_file.txt".to_string(),
        ];
        // ホームディレクトリが見つからなくても、-f を指定すれば記録できる
        let (file_path, _) =
            parse_arguments_with(&args, || Err(HOME_DIR_NOT_FOUND_MSG.to_string())).unwrap();
        assert_eq!(file_path, Path::new("custom_file.txt"));
    }

    #[test]
    fn test_parse_arguments_default_path_error() {
        let args = vec!["program_name".to_string(), "stop".to_string()];
        let result = parse_arguments_with(&args, || Err(HOME_DIR_NOT_FOUND_MSG.to_string()));
        assert_eq!(result.unwrap_err(), HOME_DIR_NOT_FOUND_MSG);
    }

    #[test]
    fn test_resolve_record_path_prefers_env_value() {
        let path = resolve_record_path(Some("env_record.txt".into()), Some(PathBuf::from("home")))
            .unwrap();
        assert_eq!(path, Path::new("env_record.txt"));
        // ホームディレクトリが見つからなくても、環境変数があれば記録できる
        let path = resolve_record_path(Some("env_record.txt".into()), None).unwrap();
        assert_eq!(path, Path::new("env_record.txt"));
    }

    #[test]
    fn test_resolve_record_path_uses_home_dir() {
        let home = PathBuf::from("home");
        let path = resolve_record_path(None, Some(home.clone())).unwrap();
        assert_eq!(path, home.join("working_time_record.txt"));
    }

    #[test]
    fn test_resolve_record_path_without_home_dir() {
        assert_eq!(
            resolve_record_path(None, None).unwrap_err(),
            HOME_DIR_NOT_FOUND_MSG
        );
        assert_eq!(
            resolve_record_path(None, Some(PathBuf::new())).unwrap_err(),
            HOME_DIR_NOT_FOUND_MSG
        );
    }

    #[test]
    fn test_parse_arguments_missing_file_argument() {
        let args = vec![
            "program_name".to_string(),
            "start".to_string(),
            "-f".to_string(),
        ];
        let result = parse_arguments(&args);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), FILENAME_NOT_PROVIDED_MSG);
    }
}
