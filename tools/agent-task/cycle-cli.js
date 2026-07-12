import { main } from "./cycle-main.js";

process.exitCode = await main(process.argv.slice(2));
