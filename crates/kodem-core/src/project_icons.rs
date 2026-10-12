//! Built-in project glyphs, grouped for the picker, and imported project images.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Passes the table of built-in project glyphs to the macro `$consumer`.
///
/// Each group has a picker label. Each row names a `ProjectIcon` variant, its vector in
/// `assets/icons/projects/` and its picker label. A saved project stores the variant name, so a
/// variant must keep its name.
#[macro_export]
macro_rules! with_project_icons {
    ($consumer:ident) => {
        $consumer! {
            "Code" {
                Folder => ("folder", "Folder"),
                FolderGit => ("folder-git-2", "Repository"),
                FolderCode => ("folder-code", "Source folder"),
                Code => ("code", "Code"),
                Braces => ("braces", "Braces"),
                Terminal => ("square-terminal", "Terminal"),
                File => ("file-text", "File"),
                FileCode => ("file-code", "Source file"),
                Branch => ("git-branch", "Branch"),
                Merge => ("git-merge", "Merge"),
                PullRequest => ("git-pull-request", "Pull request"),
                Commit => ("git-commit-horizontal", "Commit"),
                Bug => ("bug", "Bug"),
                Binary => ("binary", "Binary"),
                Regex => ("regex", "Regex"),
                Variable => ("variable", "Variable"),
                Hash => ("hash", "Hash"),
                Command => ("command", "Command"),
                Keyboard => ("keyboard", "Keyboard"),
                Workflow => ("workflow", "Workflow"),
                Webhook => ("webhook", "Webhook"),
                Component => ("component", "Component"),
                Blocks => ("blocks", "Blocks"),
                Puzzle => ("puzzle", "Plugin"),
                Package => ("package", "Package"),
                Cube => ("box", "Cube"),
                Boxes => ("boxes", "Monorepo"),
                Layers => ("layers", "Layers"),
                Library => ("library", "Library"),
            }
            "Systems" {
                Server => ("server", "Server"),
                Database => ("database", "Database"),
                HardDrive => ("hard-drive", "Storage"),
                Cloud => ("cloud", "Cloud"),
                Container => ("container", "Container"),
                Network => ("network", "Network"),
                Router => ("router", "Router"),
                Wifi => ("wifi", "Wireless"),
                Satellite => ("satellite", "Satellite"),
                RadioTower => ("radio-tower", "Antenna"),
                Cpu => ("cpu", "Processor"),
                CircuitBoard => ("circuit-board", "Circuit"),
                Microchip => ("microchip", "Chip"),
                Plug => ("plug", "Plug"),
                Cable => ("cable", "Cable"),
                Gauge => ("gauge", "Gauge"),
                Activity => ("activity", "Monitoring"),
                Shield => ("shield", "Shield"),
                Lock => ("lock", "Lock"),
                Key => ("key-round", "Key"),
                Fingerprint => ("fingerprint", "Fingerprint"),
                Wrench => ("wrench", "Tools"),
                Hammer => ("hammer", "Build"),
                Cog => ("cog", "Gear"),
                Bot => ("bot", "Robot"),
                Brain => ("brain", "Brain"),
                BrainCircuit => ("brain-circuit", "Neural network"),
                Sparkles => ("sparkles", "AI"),
            }
            "Apps" {
                Monitor => ("monitor", "Desktop"),
                Laptop => ("laptop", "Laptop"),
                Smartphone => ("smartphone", "Phone"),
                Tablet => ("tablet", "Tablet"),
                Watch => ("watch", "Watch"),
                Tv => ("tv", "TV"),
                AppWindow => ("app-window", "Window"),
                PanelsTopLeft => ("panels-top-left", "Layout"),
                Dashboard => ("layout-dashboard", "Dashboard"),
                Globe => ("globe", "Web"),
                Earth => ("earth", "Earth"),
                Link => ("link", "Link"),
                Search => ("search", "Search"),
                Mail => ("mail", "Mail"),
                Message => ("message-square", "Chat"),
                Bell => ("bell", "Notifications"),
                Calendar => ("calendar", "Calendar"),
                Clock => ("clock", "Clock"),
                Map => ("map", "Map"),
                MapPin => ("map-pin", "Location"),
                Compass => ("compass", "Compass"),
                Bookmark => ("bookmark", "Bookmark"),
                Tag => ("tag", "Tag"),
                Users => ("users", "Team"),
                User => ("user", "Profile"),
                Rss => ("rss", "Feed"),
                Inbox => ("inbox", "Inbox"),
            }
            "Business" {
                ChartLine => ("chart-line", "Line chart"),
                ChartColumn => ("chart-column", "Bar chart"),
                ChartPie => ("chart-pie", "Pie chart"),
                Table => ("table", "Table"),
                Calculator => ("calculator", "Calculator"),
                Banknote => ("banknote", "Money"),
                Coins => ("coins", "Coins"),
                Wallet => ("wallet", "Wallet"),
                CreditCard => ("credit-card", "Payments"),
                ShoppingCart => ("shopping-cart", "Shop"),
                Store => ("store", "Store"),
                Briefcase => ("briefcase", "Work"),
                Building => ("building-2", "Company"),
                Landmark => ("landmark", "Bank"),
                Receipt => ("receipt", "Receipt"),
                Truck => ("truck", "Delivery"),
                Factory => ("factory", "Factory"),
                Scale => ("scale", "Legal"),
                Handshake => ("handshake", "Handshake"),
                Megaphone => ("megaphone", "Marketing"),
                Presentation => ("presentation", "Presentation"),
            }
            "Creative" {
                Palette => ("palette", "Palette"),
                Brush => ("brush", "Brush"),
                PenTool => ("pen-tool", "Vector"),
                Pencil => ("pencil", "Pencil"),
                Ruler => ("pencil-ruler", "Design"),
                Shapes => ("shapes", "Shapes"),
                Type => ("type", "Typography"),
                Picture => ("image", "Picture"),
                Camera => ("camera", "Camera"),
                Film => ("film", "Film"),
                Clapperboard => ("clapperboard", "Video"),
                Music => ("music", "Music"),
                Headphones => ("headphones", "Audio"),
                Mic => ("mic", "Microphone"),
                AudioWaveform => ("audio-waveform", "Waveform"),
                Piano => ("piano", "Piano"),
                Guitar => ("guitar", "Guitar"),
                Aperture => ("aperture", "Aperture"),
                Scissors => ("scissors", "Scissors"),
                Feather => ("feather", "Writing"),
                Newspaper => ("newspaper", "News"),
            }
            "Games" {
                Gamepad => ("gamepad-2", "Gamepad"),
                Joystick => ("joystick", "Joystick"),
                Dice => ("dices", "Dice"),
                Swords => ("swords", "Swords"),
                Sword => ("sword", "Sword"),
                Axe => ("axe", "Axe"),
                Pickaxe => ("pickaxe", "Pickaxe"),
                Crown => ("crown", "Crown"),
                Trophy => ("trophy", "Trophy"),
                Medal => ("medal", "Medal"),
                Ghost => ("ghost", "Ghost"),
                Skull => ("skull", "Skull"),
                Castle => ("castle", "Castle"),
                Wand => ("wand-sparkles", "Magic"),
                Gem => ("gem", "Gem"),
                Target => ("target", "Target"),
                Crosshair => ("crosshair", "Crosshair"),
                Flag => ("flag", "Flag"),
                Rocket => ("rocket", "Rocket"),
                Zap => ("zap", "Lightning"),
                Flame => ("flame", "Fire"),
                Bomb => ("bomb", "Bomb"),
                Heart => ("heart", "Heart"),
                Star => ("star", "Star"),
                Spade => ("spade", "Spade"),
                Club => ("club", "Club"),
                Diamond => ("diamond", "Diamond"),
                ChessKnight => ("chess-knight", "Chess"),
            }
            "Science" {
                Book => ("book-open", "Book"),
                Notebook => ("notebook", "Notebook"),
                Graduation => ("graduation-cap", "Education"),
                Flask => ("flask-conical", "Lab"),
                TestTube => ("test-tube", "Test tube"),
                Atom => ("atom", "Atom"),
                Dna => ("dna", "DNA"),
                Microscope => ("microscope", "Microscope"),
                Telescope => ("telescope", "Telescope"),
                Orbit => ("orbit", "Orbit"),
                Lightbulb => ("lightbulb", "Idea"),
                Sigma => ("sigma", "Math"),
                Pi => ("pi", "Pi"),
                Infinity => ("infinity", "Infinity"),
                Magnet => ("magnet", "Magnet"),
                Radiation => ("radiation", "Radiation"),
                Stethoscope => ("stethoscope", "Medical"),
                Pill => ("pill", "Pill"),
                HeartPulse => ("heart-pulse", "Health"),
                Thermometer => ("thermometer", "Thermometer"),
                Languages => ("languages", "Translation"),
            }
            "World" {
                Leaf => ("leaf", "Leaf"),
                Sprout => ("sprout", "Sprout"),
                Tree => ("tree-pine", "Tree"),
                Flower => ("flower-2", "Flower"),
                Mountain => ("mountain", "Mountain"),
                Sun => ("sun", "Sun"),
                Moon => ("moon", "Moon"),
                Snowflake => ("snowflake", "Snow"),
                Droplet => ("droplet", "Water"),
                Waves => ("waves", "Waves"),
                Wind => ("wind", "Wind"),
                Cat => ("cat", "Cat"),
                Dog => ("dog", "Dog"),
                Bird => ("bird", "Bird"),
                Fish => ("fish", "Fish"),
                Rabbit => ("rabbit", "Rabbit"),
                Turtle => ("turtle", "Turtle"),
                Paw => ("paw-print", "Paw"),
                Home => ("house", "Home"),
                Tent => ("tent", "Tent"),
                Anchor => ("anchor", "Anchor"),
                Plane => ("plane", "Plane"),
                Car => ("car", "Car"),
                Bike => ("bike", "Bike"),
                Ship => ("ship", "Ship"),
                Train => ("train-front", "Train"),
            }
            "Objects" {
                Coffee => ("coffee", "Coffee"),
                Pizza => ("pizza", "Pizza"),
                Cake => ("cake-slice", "Cake"),
                Apple => ("apple", "Apple"),
                Cherry => ("cherry", "Cherry"),
                Candy => ("candy", "Candy"),
                Beer => ("beer", "Beer"),
                Gift => ("gift", "Gift"),
                Umbrella => ("umbrella", "Umbrella"),
                Glasses => ("glasses", "Glasses"),
                Hourglass => ("hourglass", "Hourglass"),
                Lamp => ("lamp-desk", "Lamp"),
                Dumbbell => ("dumbbell", "Fitness"),
                Shirt => ("shirt", "Shirt"),
                Printer => ("printer", "Printer"),
                Telephone => ("phone", "Telephone"),
                Lifebuoy => ("life-buoy", "Support"),
                Ticket => ("ticket", "Ticket"),
                Pin => ("pin", "Pin"),
                Eye => ("eye", "Eye"),
                Archive => ("archive", "Archive"),
                Hexagon => ("hexagon", "Hexagon"),
                Triangle => ("triangle", "Triangle"),
                CircleDot => ("circle-dot", "Dot"),
            }
        }
    };
}

// Expands the glyph table into `ProjectIcon` and its lookups.
macro_rules! project_icon {
    ($($group:literal { $($variant:ident => ($asset:literal, $label:literal),)+ })+) => {
        /// The automatic choice, a built-in project glyph, or an imported image stored by the
        /// local host.
        #[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
        pub enum ProjectIcon {
            #[default]
            Auto,
            $($($variant,)+)+
            Image(PathBuf),
        }
        impl ProjectIcon {
            /// The built-in glyphs in picker order: the label of each group with its icons.
            pub const GROUPS: &'static [(&'static str, &'static [Self])] =
                &[$(($group, &[$(Self::$variant,)+]),)+];
            /// Returns the picker label for this icon.
            pub fn label(&self) -> &'static str {
                match self {
                    Self::Auto => "Automatic",
                    $($(Self::$variant => $label,)+)+
                    Self::Image(_) => "Custom image",
                }
            }
            /// Returns the bundled asset, or lets the desktop choose its automatic glyph.
            pub fn glyph(&self) -> Option<&'static str> {
                match self {
                    Self::Auto | Self::Image(_) => None,
                    $($(Self::$variant => Some(concat!("projects/", $asset)),)+)+
                }
            }
        }
    };
}
with_project_icons!(project_icon);
