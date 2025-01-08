use chrono::{Local, SecondsFormat};
use std::env;
use std::fs::OpenOptions;
use std::io::Write;

const TASK_NAME_NOT_PROVIDED_MSG: &str = "タスク名が提供されていません。";
const FILENAME_NOT_PROVIDED_MSG: &str = "ファイル名が指定されていません";

fn main() {
    let args: Vec<String> = env::args().collect();
    let err = execute(&args);
    if let Err(e) = err {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn execute(args: &[String]) -> Result<(), String> {
    if args.len() < 2 {
        return Err("No subcommand provided.".to_string());
    }

    match args[1].as_str() {
        "help" => {
            display_help();
            Ok(())
        }
        "start" => handle_start_command(&args),
        "stop" => handle_stop_command(&args),
        _ => Err(format!("Invalid subcommand '{}'.", args[1])),
    }
}

fn display_help() {
    println!("Usage:");
    println!("  start <task_name> [-f <file>]    Start tracking time for a task.");
    println!("  stop                             Stop tracking time.");
    println!("  help                             Display this help message.");
}

fn handle_start_command(args: &[String]) -> Result<(), String> {
    let (file_path, remaining_args) = parse_arguments(args)?;
    let timestamp = get_current_time();

    if remaining_args.is_empty() {
        return Err(TASK_NAME_NOT_PROVIDED_MSG.into());
    }

    let task_name = remaining_args[0].as_str();
    let record = format!("{}\tstart\t{}\n", timestamp, task_name);
    write_to_file(&file_path, &record)
}

fn handle_stop_command(args: &[String]) -> Result<(), String> {
    let (file_path, _remaining_args) = parse_arguments(args)?;
    let timestamp = get_current_time();
    let record = format!("{}\tstop\t\n", timestamp);
    write_to_file(&file_path, &record)
}

// 共通の引数処理関数
fn parse_arguments(args: &[String]) -> Result<(String, Vec<String>), String> {
    let mut file_path = get_working_time_record_path();
    let mut remaining_args = Vec::new();
    let mut iter = args.iter().skip(2);

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-f" | "--file" => file_path = iter.next().ok_or(FILENAME_NOT_PROVIDED_MSG)?.clone(),
            _ => remaining_args.push(arg.clone()),
        }
    }

    Ok((file_path, remaining_args))
}

fn get_current_time() -> String {
    Local::now().to_rfc3339_opts(SecondsFormat::Secs, false)
}

fn get_working_time_record_path() -> String {
    env::var("WORKING_TIME_RECORD").unwrap_or_else(|_| {
        dirs::home_dir()
            .expect("ホームディレクトリが見つかりません")
            .join("working_time_record.txt")
            .to_str()
            .unwrap()
            .to_string()
    })
}

fn write_to_file(file_path: &str, content: &str) -> Result<(), String> {
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

    fn setup_test_file() -> String {
        let test_file = "test_working_time_record.txt";
        if fs::metadata(test_file).is_ok() {
            fs::remove_file(test_file).unwrap();
        }
        test_file.to_string()
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
        let test_file = setup_test_file();
        let args = vec![
            "program_name".to_string(),
            "start".to_string(),
            "test_task".to_string(),
            "-f".to_string(),
            test_file.clone(),
        ];
        assert!(handle_start_command(&args).is_ok());
        let content = fs::read_to_string(&test_file).unwrap();
        assert!(content.contains("start\ttest_task"));
        fs::remove_file(test_file).unwrap();
    }

    #[test]
    fn test_handle_start_command_missing_task_name() {
        let test_file = setup_test_file();
        let args = vec![
            "program_name".to_string(),
            "start".to_string(),
            "-f".to_string(),
            test_file.clone(),
        ];
        let result = handle_start_command(&args);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), TASK_NAME_NOT_PROVIDED_MSG);
    }

    #[test]
    fn test_handle_stop_command() {
        let test_file = setup_test_file();
        let args = vec![
            "program_name".to_string(),
            "stop".to_string(),
            "-f".to_string(),
            test_file.clone(),
        ];
        assert!(handle_stop_command(&args).is_ok());
        let content = fs::read_to_string(&test_file).unwrap();
        assert!(content.contains("stop"));
        fs::remove_file(test_file).unwrap();
    }

    #[test]
    fn test_parse_arguments_default_file_path() {
        let args = vec![
            "program_name".to_string(),
            "start".to_string(),
            "test_task".to_string(),
        ];
        let (file_path, remaining_args) = parse_arguments(&args).unwrap();
        assert!(file_path.contains("working_time_record.txt"));
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
        assert_eq!(file_path, "custom_file.txt");
        assert_eq!(remaining_args, vec!["test_task".to_string()]);
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
