// A static CommonJS require lets both Node and Bun's executable bundler load the native addon.
module.exports = require("../../native/electropic.node");
