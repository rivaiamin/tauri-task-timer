module.exports = {
  apps: [
    {
      name: "task-timer",
      script: "./build/index.js",
      env: {
        NODE_ENV: "production",
        PORT: 3001,
        ORIGIN: "http://task-timer.test"
      }
    }
  ]
};
