//! ELIZA chatbot implementation.
//!
//! A simple (and not very convincing) simulation of a psychotherapist.
//! It emulates the DOCTOR script written for Joseph Weizenbaum's 1966
//! ELIZA natural language processing system.
//!
//! Ported from connectrpc/examples-go's `internal/eliza` package, which is
//! itself adapted from <https://github.com/mattshiel/eliza-go>.

use rand::Rng;
use regex::Regex;
use std::sync::LazyLock;

/// Process user input and return a response.
/// Returns the response string and whether the session should end.
pub fn reply(input: &str) -> (String, bool) {
    let input = preprocess(input);
    if GOODBYE_INPUTS.contains(&input.as_str()) {
        return (random_element_from(&GOODBYE_RESPONSES), true);
    }
    (lookup_response(&input), false)
}

/// Generate introductory responses for the given name.
pub fn get_intro_responses(name: &str) -> Vec<String> {
    let mut intros: Vec<String> = INTRO_RESPONSES
        .iter()
        .map(|template| template.replace("{name}", name))
        .collect();
    intros.push(random_element_from(&ELIZA_FACTS));
    intros.push("How are you feeling today?".to_string());
    intros
}

fn lookup_response(input: &str) -> String {
    for (re, responses) in PATTERNS.iter() {
        if let Some(caps) = re.captures(input) {
            let response = random_element_from(responses);
            if !response.contains("{0}") {
                return response;
            }
            if let Some(fragment) = caps.get(1) {
                let reflected = reflect(fragment.as_str());
                return response.replace("{0}", &reflected);
            }
        }
    }
    random_element_from(&DEFAULT_RESPONSES)
}

fn preprocess(input: &str) -> String {
    input
        .trim()
        .to_lowercase()
        .trim_matches(|c: char| matches!(c, '.' | '!' | '?' | '\'' | '"'))
        .to_string()
}

fn reflect(fragment: &str) -> String {
    fragment
        .split_whitespace()
        .map(|word| {
            REFLECTED_WORDS
                .iter()
                .find(|(from, _)| *from == word)
                .map(|(_, to)| *to)
                .unwrap_or(word)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn random_element_from(list: &[String]) -> String {
    let mut rng = rand::rng();
    list[rng.random_range(0..list.len())].clone()
}

// ============================================================================
// Data tables
// ============================================================================

static GOODBYE_INPUTS: &[&str] = &["bye", "exit", "goodbye", "quit"];

static GOODBYE_RESPONSES: LazyLock<Vec<String>> = LazyLock::new(|| {
    vec![
        "Goodbye. It was nice talking to you.".into(),
        "Thank you for talking with me.".into(),
        "Thank you, that will be $150. Have a good day!".into(),
        "Goodbye. This was really a nice talk.".into(),
        "Goodbye. I'm looking forward to our next session.".into(),
        "This was a good session, wasn't it - but time is over now. Goodbye.".into(),
        "Maybe we could discuss this over more in our next session? Goodbye.".into(),
        "Good-bye.".into(),
    ]
});

static DEFAULT_RESPONSES: LazyLock<Vec<String>> = LazyLock::new(|| {
    vec![
        "Please tell me more.".into(),
        "Let's change focus a bit...Tell me about your family.".into(),
        "Can you elaborate on that?".into(),
        "I see.".into(),
        "Very interesting.".into(),
        "I see. And what does that tell you?".into(),
        "How does that make you feel?".into(),
        "How do you feel when you say that?".into(),
    ]
});

static INTRO_RESPONSES: LazyLock<Vec<String>> = LazyLock::new(|| {
    vec![
        "Hi {name}. I'm Eliza.".into(),
        "Before we begin, {name}, let me tell you something about myself.".into(),
    ]
});

static ELIZA_FACTS: LazyLock<Vec<String>> = LazyLock::new(|| {
    vec![
        "I was created by Joseph Weizenbaum.".into(),
        "I was created in the 1960s.".into(),
        "I am a Rogerian psychotherapist.".into(),
        "I am named after Eliza Doolittle from the play Pygmalion.".into(),
        "I was originally written on an IBM 7094.".into(),
        "I can be accessed in most Emacs implementations with the command M-x doctor.".into(),
        "I was created at the MIT Artificial Intelligence Laboratory.".into(),
        "I was one of the first programs capable of attempting the Turing test.".into(),
        "I was designed as a method to show the superficiality of communication between man and machine.".into(),
    ]
});

static REFLECTED_WORDS: &[(&str, &str)] = &[
    ("am", "are"),
    ("was", "were"),
    ("i", "you"),
    ("i'd", "you would"),
    ("i've", "you have"),
    ("i'll", "you will"),
    ("my", "your"),
    ("are", "am"),
    ("you've", "I have"),
    ("you'll", "I will"),
    ("your", "my"),
    ("yours", "mine"),
    ("you", "me"),
    ("me", "you"),
];

static PATTERNS: LazyLock<Vec<(Regex, Vec<String>)>> = LazyLock::new(|| {
    vec![
        (
            Regex::new(r"i need (.*)").unwrap(),
            vec![
                "Why do you need {0}?".into(),
                "Would it really help you to get {0}?".into(),
                "Are you sure you need {0}?".into(),
            ],
        ),
        (
            Regex::new(r"why don'?t you ([^\?]*)\??").unwrap(),
            vec![
                "Do you really think I don't {0}?".into(),
                "Perhaps eventually I will {0}.".into(),
                "Do you really want me to {0}?".into(),
            ],
        ),
        (
            Regex::new(r"why can'?t I ([^\?]*)\??").unwrap(),
            vec![
                "Do you think you should be able to {0}?".into(),
                "If you could {0}, what would you do?".into(),
                "I don't know -- why can't you {0}?".into(),
                "Have you really tried?".into(),
            ],
        ),
        (
            Regex::new(r"i can'?t (.*)").unwrap(),
            vec![
                "How do you know you can't {0}?".into(),
                "Perhaps you could {0} if you tried.".into(),
                "What would it take for you to {0}?".into(),
            ],
        ),
        (
            Regex::new(r"i am (.*)").unwrap(),
            vec![
                "Did you come to me because you are {0}?".into(),
                "How long have you been {0}?".into(),
                "How do you feel about being {0}?".into(),
            ],
        ),
        (
            Regex::new(r"i'?m (.*)").unwrap(),
            vec![
                "How does being {0} make you feel?".into(),
                "Do you enjoy being {0}?".into(),
                "Why do you tell me you're {0}?".into(),
                "Why do you think you're {0}?".into(),
            ],
        ),
        (
            Regex::new(r"are you ([^\?]*)\??").unwrap(),
            vec![
                "Why does it matter whether I am {0}?".into(),
                "Would you prefer it if I were not {0}?".into(),
                "Perhaps you believe I am {0}.".into(),
                "I may be {0} -- what do you think?".into(),
            ],
        ),
        (
            Regex::new(r"what (.*)").unwrap(),
            vec![
                "Why do you ask?".into(),
                "How would an answer to that help you?".into(),
                "What do you think?".into(),
            ],
        ),
        (
            Regex::new(r"how (.*)").unwrap(),
            vec![
                "How do you suppose?".into(),
                "Perhaps you can answer your own question.".into(),
                "What is it you're really asking?".into(),
            ],
        ),
        (
            Regex::new(r"because (.*)").unwrap(),
            vec![
                "Is that the real reason?".into(),
                "What other reasons come to mind?".into(),
                "Does that reason apply to anything else?".into(),
                "If {0}, what else must be true?".into(),
            ],
        ),
        (
            Regex::new(r"(.*) sorry (.*)").unwrap(),
            vec![
                "There are many times when no apology is needed.".into(),
                "What feelings do you have when you apologize?".into(),
            ],
        ),
        (
            Regex::new(r"^hello(.*)").unwrap(),
            vec![
                "Hello...I'm glad you could drop by today.".into(),
                "Hello there...how are you today?".into(),
                "Hello, how are you feeling today?".into(),
            ],
        ),
        (
            Regex::new(r"^hi(.*)").unwrap(),
            vec![
                "Hello...I'm glad you could drop by today.".into(),
                "Hi there...how are you today?".into(),
                "Hello, how are you feeling today?".into(),
            ],
        ),
        (
            Regex::new(r"^thanks(.*)").unwrap(),
            vec!["You're welcome!".into(), "Anytime!".into()],
        ),
        (
            Regex::new(r"^thank you(.*)").unwrap(),
            vec!["You're welcome!".into(), "Anytime!".into()],
        ),
        (
            Regex::new(r"^good morning(.*)").unwrap(),
            vec![
                "Good morning...I'm glad you could drop by today.".into(),
                "Good morning...how are you today?".into(),
                "Good morning, how are you feeling today?".into(),
            ],
        ),
        (
            Regex::new(r"^good afternoon(.*)").unwrap(),
            vec![
                "Good afternoon...I'm glad you could drop by today.".into(),
                "Good afternoon...how are you today?".into(),
                "Good afternoon, how are you feeling today?".into(),
            ],
        ),
        (
            // Note: connectrpc/examples-go's globals.go has uppercase `I`
            // here — a bug on their side too, since preprocess() lowercases
            // input. We fix ours.
            Regex::new(r"i think (.*)").unwrap(),
            vec![
                "Do you doubt {0}?".into(),
                "Do you really think so?".into(),
                "But you're not sure {0}?".into(),
            ],
        ),
        (
            Regex::new(r"(.*) friend (.*)").unwrap(),
            vec![
                "Tell me more about your friends.".into(),
                "When you think of a friend, what comes to mind?".into(),
                "Why don't you tell me about a childhood friend?".into(),
            ],
        ),
        (
            Regex::new(r"yes").unwrap(),
            vec![
                "You seem quite sure.".into(),
                "OK, but can you elaborate a bit?".into(),
            ],
        ),
        (
            Regex::new(r"(.*) computer(.*)").unwrap(),
            vec![
                "Are you really talking about me?".into(),
                "Does it seem strange to talk to a computer?".into(),
                "How do computers make you feel?".into(),
                "Do you feel threatened by computers?".into(),
            ],
        ),
        (
            Regex::new(r"is it (.*)").unwrap(),
            vec![
                "Do you think it is {0}?".into(),
                "Perhaps it's {0} -- what do you think?".into(),
                "If it were {0}, what would you do?".into(),
                "It could well be that {0}.".into(),
            ],
        ),
        (
            Regex::new(r"it is (.*)").unwrap(),
            vec![
                "You seem very certain.".into(),
                "If I told you that it probably isn't {0}, what would you feel?".into(),
            ],
        ),
        (
            Regex::new(r"can you ([^\?]*)\??").unwrap(),
            vec![
                "What makes you think I can't {0}?".into(),
                "If I could {0}, then what?".into(),
                "Why do you ask if I can {0}?".into(),
            ],
        ),
        (
            Regex::new(r"(.*)dream(.*)").unwrap(),
            vec!["Tell me more about your dream.".into()],
        ),
        (
            Regex::new(r"can I ([^\?]*)\??").unwrap(),
            vec![
                "Perhaps you don't want to {0}.".into(),
                "Do you want to be able to {0}?".into(),
                "If you could {0}, would you?".into(),
            ],
        ),
        (
            Regex::new(r"you are (.*)").unwrap(),
            vec![
                "Why do you think I am {0}?".into(),
                "Does it please you to think that I'm {0}?".into(),
                "Perhaps you would like me to be {0}.".into(),
                "Perhaps you're really talking about yourself?".into(),
            ],
        ),
        (
            Regex::new(r"you'?re (.*)").unwrap(),
            vec![
                "Why do you say I am {0}?".into(),
                "Why do you think I am {0}?".into(),
                "Are we talking about you, or me?".into(),
            ],
        ),
        (
            Regex::new(r"i don'?t (.*)").unwrap(),
            vec![
                "Don't you really {0}?".into(),
                "Why don't you {0}?".into(),
                "Do you want to {0}?".into(),
            ],
        ),
        (
            Regex::new(r"i feel (.*)").unwrap(),
            vec![
                "Good, tell me more about these feelings.".into(),
                "Do you often feel {0}?".into(),
                "When do you usually feel {0}?".into(),
                "When you feel {0}, what do you do?".into(),
                "Feeling {0}? Tell me more.".into(),
            ],
        ),
        (
            Regex::new(r"i have (.*)").unwrap(),
            vec![
                "Why do you tell me that you've {0}?".into(),
                "Have you really {0}?".into(),
                "Now that you have {0}, what will you do next?".into(),
            ],
        ),
        (
            Regex::new(r"i would (.*)").unwrap(),
            vec![
                "Could you explain why you would {0}?".into(),
                "Why would you {0}?".into(),
                "Who else knows that you would {0}?".into(),
            ],
        ),
        (
            Regex::new(r"is there (.*)").unwrap(),
            vec![
                "Do you think there is {0}?".into(),
                "It's likely that there is {0}.".into(),
                "Would you like there to be {0}?".into(),
            ],
        ),
        (
            Regex::new(r"my (.*)").unwrap(),
            vec![
                "I see, your {0}.".into(),
                "Why do you say that your {0}?".into(),
                "When your {0}, how do you feel?".into(),
            ],
        ),
        (
            Regex::new(r"you (.*)").unwrap(),
            vec![
                "We should be discussing you, not me.".into(),
                "Why do you say that about me?".into(),
                "Why do you care whether I {0}?".into(),
            ],
        ),
        (
            Regex::new(r"why (.*)").unwrap(),
            vec![
                "Why don't you tell me the reason why {0}?".into(),
                "Why do you think {0}?".into(),
            ],
        ),
        (
            Regex::new(r"i want (.*)").unwrap(),
            vec![
                "What would it mean to you if you got {0}?".into(),
                "Why do you want {0}?".into(),
                "What would you do if you got {0}?".into(),
                "If you got {0}, then what would you do?".into(),
            ],
        ),
        (
            Regex::new(r"(.*) mother(.*)").unwrap(),
            vec![
                "Tell me more about your mother.".into(),
                "What was your relationship with your mother like?".into(),
                "How do you feel about your mother?".into(),
                "How does this relate to your feelings today?".into(),
                "Good family relations are important.".into(),
            ],
        ),
        (
            Regex::new(r"(.*) father(.*)").unwrap(),
            vec![
                "Tell me more about your father.".into(),
                "How did your father make you feel?".into(),
                "How do you feel about your father?".into(),
                "Does your relationship with your father relate to your feelings today?".into(),
                "Do you have trouble showing affection with your family?".into(),
            ],
        ),
        (
            Regex::new(r"(.*) child(.*)").unwrap(),
            vec![
                "Did you have close friends as a child?".into(),
                "What is your favorite childhood memory?".into(),
                "Do you remember any dreams or nightmares from childhood?".into(),
                "Did the other children sometimes tease you?".into(),
                "How do you think your childhood experiences relate to your feelings today?".into(),
            ],
        ),
        (
            Regex::new(r"(.*)\?").unwrap(),
            vec![
                "Why do you ask that?".into(),
                "Please consider whether you can answer your own question.".into(),
                "Perhaps the answer lies within yourself?".into(),
                "Why don't you tell me?".into(),
            ],
        ),
    ]
});
