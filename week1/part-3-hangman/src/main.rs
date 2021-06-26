// Simple Hangman Program
// User gets five incorrect guesses
// Word chosen randomly from words.txt
// Inspiration from: https://doc.rust-lang.org/book/ch02-00-guessing-game-tutorial.html
// This assignment will introduce you to some fundamental syntax in Rust:
// - variable declaration
// - string manipulation
// - conditional statements
// - loops
// - vectors
// - files
// - user input
// We've tried to limit/hide Rust's quirks since we'll discuss those details
// more in depth in the coming lectures.
extern crate rand;
use rand::Rng;
use std::fs;
use std::io;
use std::io::Write;

const NUM_INCORRECT_GUESSES: u32 = 5;
const WORDS_PATH: &str = "words.txt";

fn pick_a_random_word() -> String {
    let file_string = fs::read_to_string(WORDS_PATH).expect("Unable to read file.");
    let words: Vec<&str> = file_string.split('\n').collect();
    String::from(words[rand::thread_rng().gen_range(0, words.len())].trim())
}

fn print_word_so_far(revealed_indexes: &Vec<bool>, secret_word_chars: &Vec<char>) {
    let mut s: String = String::new();
    for i in 0..secret_word_chars.len() {
        if revealed_indexes[i] {
            s.push(secret_word_chars[i]);
        } else {
            s.push('-');
        }
    }

    println!("The word so far is {}", s);
}

fn print_guessed_so_far(guessed_so_far: &Vec<char>) {
    let mut s: String = String::new();
    for i in 0..guessed_so_far.len() {
        s.push(guessed_so_far[i]);
    }
    println!("You have guessed the following letters: {}", s);
}

fn print_num_guess_left(num_guess_left: u32) {
    println!("You have {} guesses left", num_guess_left);
}

fn get_guess() -> char {
    print!("Please guess a letter: ");
    io::stdout().flush().expect("Error flushing stdout.");

    let mut guess = String::new();
    io::stdin()
        .read_line(&mut guess)
        .expect("Error reading line.");
    guess.chars().next().unwrap()
}

fn process_user_input(
    revealed_indexes: &mut Vec<bool>,
    secret_word_chars: &Vec<char>,
    c: char,
    num_guess_left: u32,
) -> u32 {
    let mut found: bool = false;
    let mut returnv = num_guess_left;

    for i in 0..secret_word_chars.len() {
        if secret_word_chars[i] == c {
            revealed_indexes[i] = true;
            found = true;
        }
    }

    if !found {
        returnv -= 1;
        println!("Sorry, that letter is not in the word");
    }
    returnv
}

fn print_result(num_guess_left: u32, secret_word: &String) {
    if num_guess_left == 0 {
        println!("Sorry, you ran out of guesses!");
    } else {
        println!(
            "Congratulations you guessed the secret word: {}",
            secret_word
        );
    }
}

fn main() {
    let secret_word = pick_a_random_word();
    // Note: given what you know about Rust so far, it's easier to pull characters out of a
    // vector than it is to pull them out of a string. You can get the ith character of
    // secret_word by doing secret_word_chars[i].
    let secret_word_chars: Vec<char> = secret_word.chars().collect();
    let mut revealed_indexes: Vec<bool> = Vec::new();
    let mut guessed_so_far: Vec<char> = Vec::new();
    let mut num_guess_left = NUM_INCORRECT_GUESSES;

    for _i in 0..secret_word.len() {
        revealed_indexes.push(false);
    }

    println!("Welcome to CS110L Hangman!");

    while 0 < num_guess_left {
        print_word_so_far(&revealed_indexes, &secret_word_chars);
        print_guessed_so_far(&guessed_so_far);
        print_num_guess_left(num_guess_left);

        let c: char = get_guess();
        num_guess_left =
            process_user_input(&mut revealed_indexes, &secret_word_chars, c, num_guess_left);
        guessed_so_far.push(c);

        println!();
    }

    print_result(num_guess_left, &secret_word);
}
